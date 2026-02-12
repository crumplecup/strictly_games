//! Strictly Games MCP Server (HTTP Transport)

use anyhow::Result;
use axum::{extract::Request, Router};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    tower::{StreamableHttpServerConfig, StreamableHttpService},
};
use std::sync::Arc;
use strictly_games::server::GameServer;
use tower::ServiceBuilder;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rmcp=debug"))
        )
        .init();

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    info!("Starting Strictly Games MCP server on HTTP");
    info!(port, "Server will listen on http://localhost:{}", port);

    let session_manager = Arc::new(LocalSessionManager::default());
    
    // Factory creates new GameServer for each session
    let http_service = StreamableHttpService::new(
        || {
            debug!("Creating new GameServer instance");
            Ok(GameServer::new())
        },
        session_manager,
        StreamableHttpServerConfig::default(),
    );
    
    // Wrap service with request logging
    let app = Router::new()
        .fallback_service(ServiceBuilder::new()
            .map_request(|req: Request| {
                info!(
                    method = %req.method(),
                    uri = %req.uri(),
                    headers = ?req.headers(),
                    "Incoming HTTP request"
                );
                req
            })
            .service(tower::service_fn(move |req| {
                let mut service = http_service.clone();
                async move {
                    let result = tower::Service::call(&mut service, req).await;
                    match &result {
                        Ok(resp) => debug!(status = ?resp.status(), "Response sent"),
                        Err(e) => warn!(error = ?e, "Request failed"),
                    }
                    result
                }
            })));
    
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    info!("✅ Server ready at http://localhost:{}/", port);
    info!("📡 Accepting SSE connections");
    info!("🎮 Tools: start_game, get_board, make_move");
    info!("🔍 Trace logging enabled - all requests will be logged");
    
    axum::serve(listener, app).await?;

    Ok(())
}
