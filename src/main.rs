//! Dakera MCP Server
//!
//! Model Context Protocol server that exposes Dakera memory operations
//! as tools for AI agents. Communicates over stdio using JSON-RPC.

use tracing_subscriber::EnvFilter;

mod protocol;
mod server;
mod tools;

#[tokio::main]
async fn main() {
    // Initialize logging to stderr (stdout is reserved for MCP protocol)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("dakera_mcp=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("Dakera MCP Server starting");

    let api_url = std::env::var("DAKERA_API_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let api_key = std::env::var("DAKERA_API_KEY")
        .ok();

    tracing::info!(api_url = %api_url, has_key = api_key.is_some(), "Connecting to Dakera API");

    let server = server::McpServer::new(api_url, api_key);
    if let Err(e) = server.run().await {
        tracing::error!(error = %e, "MCP server error");
        std::process::exit(1);
    }
}
