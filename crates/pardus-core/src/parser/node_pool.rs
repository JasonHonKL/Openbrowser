//! Memory pool for DOM nodes to reduce allocation overhead
//!
//! Pre-allocates and reuses node storage to minimize heap churn
//! and improve cache locality.

use bumpalo::Bump;
use std::cell::RefCell;
use std::sync::Arc;

/// Thread-local node pool for fast allocation
pub struct NodePool {
    /// The bump allocator for node storage
    arena: Bump,
    /// Maximum capacity before reset
    max_capacity: usize,
    /// Current allocated size
    current_size: usize,
    /// Reusable node IDs
    free_nodes: RefCell<Vec<NodeId>>,
    /// Next node ID to allocate
    next_id: RefCell<u32>,
}

/// Compact node identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    pub const NULL: NodeId = NodeId(u32::MAX);
    
    pub fn index(self) -> usize {
        self.0 as usize
    }
    
    pub fn is_null(self) -> bool {
        self == Self::NULL
    }
}

impl NodePool {
    /// Create a new pool with default capacity (1MB)
    pub fn new() -> Self {
        Self::with_capacity(1024 * 1024)
    }

    /// Create a pool with specific initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            arena: Bump::with_capacity(capacity),
            max_capacity: capacity * 4, // Allow 4x growth
            current_size: 0,
            free_nodes: RefCell::new(Vec::with_capacity(1024)),
            next_id: RefCell::new(0),
        }
    }

    /// Allocate a new node ID
    pub fn allocate_id(&self) -> NodeId {
        // Try to reuse a free ID first
        if let Some(id) = self.free_nodes.borrow_mut().pop() {
            return id;
        }
        
        // Allocate new ID
        let id = NodeId(*self.next_id.borrow());
        *self.next_id.borrow_mut() += 1;
        id
    }

    /// Mark a node ID as free for reuse
    pub fn free_id(&self, id: NodeId) {
        if !id.is_null() {
            self.free_nodes.borrow_mut().push(id);
        }
    }

    /// Allocate storage in the arena
    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.arena.alloc(val)
    }

    /// Allocate slice in the arena
    pub fn alloc_slice<T: Copy>(&self, slice: &[T]) -> &[T] {
        self.arena.alloc_slice_copy(slice)
    }

    /// Check if pool needs reset
    pub fn should_reset(&self) -> bool {
        self.arena.allocated_bytes() > self.max_capacity
    }

    /// Reset the pool, freeing all allocations but keeping capacity
    pub fn reset(&mut self) {
        self.arena.reset();
        self.current_size = 0;
        self.free_nodes.borrow_mut().clear();
        *self.next_id.borrow_mut() = 0;
    }

    /// Get memory usage statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            allocated_bytes: self.arena.allocated_bytes(),
            capacity: self.max_capacity,
            free_nodes: self.free_nodes.borrow().len(),
            next_id: *self.next_id.borrow(),
        }
    }

    /// Get total allocated bytes
    pub fn allocated_bytes(&self) -> usize {
        self.arena.allocated_bytes()
    }
}

impl Default for NodePool {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub allocated_bytes: usize,
    pub capacity: usize,
    pub free_nodes: usize,
    pub next_id: u32,
}

/// Shared pool reference for multi-threaded use
#[derive(Debug, Clone)]
pub struct SharedPool {
    inner: Arc<NodePool>,
}

impl SharedPool {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(NodePool::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(NodePool::with_capacity(capacity)),
        }
    }

    pub fn allocate_id(&self) -> NodeId {
        self.inner.allocate_id()
    }

    pub fn free_id(&self, id: NodeId) {
        self.inner.free_id(id);
    }

    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.inner.alloc(val)
    }

    pub fn stats(&self) -> PoolStats {
        self.inner.stats()
    }
}

impl Default for SharedPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Fast string interning for repeated attribute names/tags
pub struct StringInterner {
    strings: RefCell<std::collections::HashMap<String, u32>>,
    ids: RefCell<Vec<String>>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            strings: RefCell::new(std::collections::HashMap::new()),
            ids: RefCell::new(Vec::with_capacity(256)),
        }
    }

    /// Intern a string and get its ID
    pub fn intern(&self, s: &str) -> u32 {
        let mut strings = self.strings.borrow_mut();
        
        if let Some(&id) = strings.get(s) {
            return id;
        }
        
        let id = self.ids.borrow().len() as u32;
        strings.insert(s.to_string(), id);
        self.ids.borrow_mut().push(s.to_string());
        id
    }

    /// Get string by ID
    pub fn get(&self, id: u32) -> Option<String> {
        self.ids.borrow().get(id as usize).cloned()
    }

    /// Clear all interned strings
    pub fn clear(&self) {
        self.strings.borrow_mut().clear();
        self.ids.borrow_mut().clear();
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_pool_allocation() {
        let pool = NodePool::new();
        
        let id1 = pool.allocate_id();
        let id2 = pool.allocate_id();
        
        assert_ne!(id1, id2);
        assert!(!id1.is_null());
    }

    #[test]
    fn test_node_pool_reuse() {
        let pool = NodePool::new();
        
        let id1 = pool.allocate_id();
        pool.free_id(id1);
        
        let id2 = pool.allocate_id();
        assert_eq!(id1, id2); // Should reuse
    }

    #[test]
    fn test_shared_pool() {
        let pool1 = SharedPool::new();
        let pool2 = pool1.clone();
        
        let id = pool1.allocate_id();
        pool2.free_id(id);
    }

    #[test]
    fn test_string_interner() {
        let interner = StringInterner::new();
        
        let id1 = interner.intern("div");
        let id2 = interner.intern("div");
        let id3 = interner.intern("span");
        
        assert_eq!(id1, id2); // Same string, same ID
        assert_ne!(id1, id3); // Different string, different ID
        
        assert_eq!(interner.get(id1), Some("div".to_string()));
    }
}
