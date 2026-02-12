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
    
    // Create SHARED SessionManager for game state (Arc for multi-request sharing)
    let game_sessions = Arc::new(strictly_games::session::SessionManager::new());
    
    // Configure for STATELESS mode (no session management required)
    let mut config = StreamableHttpServerConfig::default();
    config.stateful_mode = false;  // Simpler protocol, no session IDs needed
    debug!(?config, "HTTP service configuration");
    
    // Factory creates GameServer that shares session state
    let http_service = StreamableHttpService::new(
        move || {
            debug!("Creating new GameServer instance with shared sessions");
            Ok(GameServer::with_sessions((*game_sessions).clone()))
        },
        session_manager,
        config,
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
            .service(tower::service_fn(move |req: Request| {
                let mut service = http_service.clone();
                async move {
                    let uri = req.uri().clone();
                    let result = tower::Service::call(&mut service, req).await;
                    match &result {
                        Ok(resp) => info!(status = ?resp.status(), uri = %uri, "Response sent"),
                        Err(e) => warn!(error = ?e, uri = %uri, "Request failed"),
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
