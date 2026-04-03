use tracing_subscriber::EnvFilter;

pub fn init_logging() {
    let json = std::env::var("LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);

    let filter = if let Ok(rust_log) = std::env::var("RUST_LOG") {
        EnvFilter::new(&rust_log)
    } else if let Ok(log_level) = std::env::var("LOG_LEVEL") {
        EnvFilter::new(&log_level)
    } else {
        EnvFilter::new("warn")
    };

    let builder = tracing_subscriber::fmt();

    if json {
        builder.json().with_env_filter(filter).init();
    } else {
        builder.with_env_filter(filter).init();
    }
}
