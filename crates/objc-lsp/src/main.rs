use anyhow::Result;
use tracing_subscriber::EnvFilter;

mod capabilities;
mod dispatch;
mod server;

fn main() -> Result<()> {
    // Log to stderr (stdout is reserved for LSP JSON-RPC).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("OBJC_LSP_LOG")
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("objc-lsp starting");

    let (connection, io_threads) = lsp_server::Connection::stdio();
    server::run(connection)?;
    io_threads.join()?;

    tracing::info!("objc-lsp exiting");
    Ok(())
}
