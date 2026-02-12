//! Strictly Games MCP Server (HTTP Transport)

use anyhow::Result;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use std::sync::Arc;
use strictly_games::server::GameServer;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    info!("Starting Strictly Games MCP server on HTTP");
    info!(port, "Server will listen on http://localhost:{}", port);

    let session_manager = Arc::new(LocalSessionManager::default());
    
    // The factory just creates a new GameServer handler
    let http_service = StreamableHttpService::new(
        || Ok(GameServer::new()),
        session_manager,
        StreamableHttpServerConfig::default(),
    );
    
    let app = Router::new()
        .fallback_service(tower::service_fn(move |req| {
            let mut service = http_service.clone();
            async move {
                tower::Service::call(&mut service, req).await
            }
        }));
    
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    info!("Server ready at http://localhost:{}/", port);
    info!("Agents can connect and call make_move, get_board, start_game tools");
    
    axum::serve(listener, app).await?;

    Ok(())
}
