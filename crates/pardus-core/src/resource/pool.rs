//! HTTP/2 connection pool management

use std::collections::HashMap;
use std::time::Duration;
use parking_lot::RwLock;
use tracing::trace;

/// Pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Max connections per origin
    pub max_per_origin: usize,
    /// Idle connection timeout
    pub idle_timeout_secs: u64,
    /// Connection lifetime
    pub max_lifetime_secs: u64,
    /// Enable HTTP/2
    pub enable_h2: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_per_origin: 6,
            idle_timeout_secs: 90,
            max_lifetime_secs: 300,
            enable_h2: true,
        }
    }
}

/// Connection pool for HTTP/1 and HTTP/2
pub struct ConnectionPool {
    config: PoolConfig,
    /// Origin -> connections mapping
    connections: RwLock<HashMap<String, Vec<PooledConnection>>>,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create connection to origin
    pub async fn get_connection(
        &self,
        origin: &str,
    ) -> anyhow::Result<ConnectionHandle> {
        self.cleanup();

        // Try to get existing idle connection
        {
            let mut conns = self.connections.write();
            if let Some(origin_conns) = conns.get_mut(origin) {
                // Find idle connection
                if let Some(pos) = origin_conns.iter().position(|c| c.is_idle()) {
                    let conn = &mut origin_conns[pos];
                    conn.mark_used();
                    return Ok(ConnectionHandle {
                        origin: origin.to_string(),
                        id: conn.id,
                    });
                }

                // Check if we can create more
                if origin_conns.len() >= self.config.max_per_origin {
                    // Reuse oldest connection
                    let conn = &mut origin_conns[0];
                    conn.mark_used();
                    return Ok(ConnectionHandle {
                        origin: origin.to_string(),
                        id: conn.id,
                    });
                }
            }
        }

        // Create new connection
        self.create_connection(origin).await
    }

    /// Create new connection
    async fn create_connection(
        &self,
        origin: &str,
    ) -> anyhow::Result<ConnectionHandle> {
        trace!("creating new connection to {}", origin);

        // Parse origin for connection
        let url = url::Url::parse(origin)?;
        let _host = url.host_str().unwrap_or("localhost");
        let _port = url.port().unwrap_or(443);

        // For now, return placeholder - rquest handles actual connections
        let id = fastrand::u64(..);

        let conn = PooledConnection {
            id,
            origin: origin.to_string(),
            created_at: std::time::Instant::now(),
            last_used: std::time::Instant::now(),
            requests_in_flight: 1,
            is_h2: self.config.enable_h2,
        };

        {
            let mut conns = self.connections.write();
            conns.entry(origin.to_string())
                .or_default()
                .push(conn);
        }

        Ok(ConnectionHandle {
            origin: origin.to_string(),
            id,
        })
    }

    /// Release connection back to pool
    pub fn release_connection(&self,
        handle: ConnectionHandle,
    ) {
        let mut conns = self.connections.write();
        if let Some(origin_conns) = conns.get_mut(&handle.origin) {
            if let Some(conn) = origin_conns.iter_mut().find(|c| c.id == handle.id) {
                conn.requests_in_flight -= 1;
            }
        }
    }

    /// Clean up stale connections
    pub fn cleanup(&self) {
        let now = std::time::Instant::now();
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);
        let max_lifetime = Duration::from_secs(self.config.max_lifetime_secs);

        let mut conns = self.connections.write();
        for (_origin, origin_conns) in conns.iter_mut() {
            origin_conns.retain(|c| {
                let is_stale = now.duration_since(c.last_used) > idle_timeout
                    || now.duration_since(c.created_at) > max_lifetime;
                !is_stale
            });
        }

        // Remove empty origins
        conns.retain(|_, v| !v.is_empty());
    }

    /// Pool statistics
    pub fn stats(&self) -> PoolStats {
        let conns = self.connections.read();
        let total = conns.values().map(|v| v.len()).sum();
        let idle = conns.values()
            .flat_map(|v| v.iter())
            .filter(|c| c.is_idle())
            .count();

        PoolStats {
            total_connections: total,
            idle_connections: idle,
            active_connections: total - idle,
            origins: conns.len(),
        }
    }
}

/// Pooled connection metadata
struct PooledConnection {
    id: u64,
    #[allow(dead_code)]
    origin: String,
    created_at: std::time::Instant,
    last_used: std::time::Instant,
    requests_in_flight: usize,
    #[allow(dead_code)]
    is_h2: bool,
}

impl PooledConnection {
    fn is_idle(&self) -> bool {
        self.requests_in_flight == 0
    }

    fn mark_used(&mut self) {
        self.last_used = std::time::Instant::now();
        self.requests_in_flight += 1;
    }
}

/// Handle to a pooled connection
#[derive(Debug)]
pub struct ConnectionHandle {
    #[allow(dead_code)]
    origin: String,
    #[allow(dead_code)]
    id: u64,
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_connections: usize,
    pub idle_connections: usize,
    pub active_connections: usize,
    pub origins: usize,
}

/// HTTP/2 specific connection with stream multiplexing
pub struct H2Connection {
    #[allow(dead_code)]
    origin: String,
    stream_counter: std::sync::atomic::AtomicUsize,
    max_concurrent_streams: usize,
}

impl H2Connection {
    pub fn new(origin: String, max_streams: usize) -> Self {
        Self {
            origin,
            stream_counter: std::sync::atomic::AtomicUsize::new(0),
            max_concurrent_streams: max_streams,
        }
    }

    /// Get next available stream ID
    pub fn next_stream_id(&self) -> Option<usize> {
        let id = self.stream_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if id < self.max_concurrent_streams {
            Some(id)
        } else {
            None
        }
    }

    /// Current stream count
    pub fn stream_count(&self) -> usize {
        self.stream_counter.load(std::sync::atomic::Ordering::SeqCst)
    }
}
