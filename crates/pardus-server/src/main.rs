mod events;
mod handlers;
mod router;
mod state;
mod static_files;
mod ws;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "pardus-server", about = "Pardus browser HTTP/WebSocket server with web UI")]
struct Args {
    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 7788)]
    port: u16,

    /// Serve static files from filesystem instead of embedded (for development)
    #[arg(long)]
    dev: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("pardus_server=info,tower_http=info")
        .init();

    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);

    let server_state = state::create_state()?;
    let app = router::build_router(server_state, args.dev);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Pardus server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
