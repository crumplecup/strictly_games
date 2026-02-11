//! Strictly Games MCP Server
//!
//! Type-safe game server demonstrating operational semantics for LLM agents.

#![warn(missing_docs)]

mod games;
mod server;

use rmcp::ServiceExt;
use server::GameServer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting Strictly Games MCP server");

    // Create server and run on stdio
    let server = GameServer::new();
    
    info!("Server ready - connect via MCP protocol");
    server.serve(rmcp::transport::stdio()).await?;

    Ok(())
}
