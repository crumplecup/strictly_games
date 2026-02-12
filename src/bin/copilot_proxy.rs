//! HTTP Proxy for MCP Clients with Incomplete Accept Headers
//!
//! This proxy sits between MCP clients (copilot CLI, Claude, etc.) and the game server,
//! transforming non-compliant requests into spec-compliant ones.
//!
//! ## Problem
//! Some MCP clients (e.g., copilot CLI v0.0.407) send incorrect requests:
//! - GET instead of POST for initialization
//! - Missing `application/json` in Accept header
//!
//! ## Solution
//! Proxy detects broken requests and transforms them:
//! - GET → POST
//! - `Accept: text/event-stream` → `Accept: application/json, text/event-stream`
//! - Adds `Content-Type: application/json` if missing
//!
//! ## Usage
//! ```bash
//! # Terminal 1: Start game server
//! cargo run --bin server_http
//!
//! # Terminal 2: Start proxy
//! cargo run --bin copilot_proxy
//!
//! # Configure copilot to use proxy
//! # ~/.copilot/mcp-config.json: "url": "http://localhost:3001"
//! ```

use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Method, Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

/// Configuration for the proxy server
#[derive(Debug, Clone)]
struct ProxyConfig {
    /// Port to listen on
    proxy_port: u16,
    /// Target server URL
    target_url: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            proxy_port: 3001,
            target_url: "http://localhost:3000".to_string(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = ProxyConfig::default();

    info!("🔧 Starting MCP Proxy Workaround");
    info!("📡 Listening on: http://localhost:{}", config.proxy_port);
    info!("🎯 Forwarding to: {}", config.target_url);
    info!("💡 Transforms non-compliant MCP client requests");

    let state = Arc::new(config.clone());

    let app = Router::new()
        .route("/", any(proxy_handler))
        .route("/*path", any(proxy_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", config.proxy_port);
    let listener = TcpListener::bind(&addr).await?;

    info!("✅ Proxy ready - clients can connect");
    info!("🔍 Use RUST_LOG=debug to see request transformations");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Main proxy handler that transforms and forwards requests
async fn proxy_handler(
    State(config): State<Arc<ProxyConfig>>,
    mut req: Request<Body>,
) -> impl IntoResponse {
    let original_method = req.method().clone();
    let original_uri = req.uri().clone();
    let client_ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    debug!(
        method = %original_method,
        uri = %original_uri,
        user_agent = client_ua,
        "Incoming request from client"
    );

    // Check if request needs transformation
    let needs_transform = needs_transformation(&req);

    if needs_transform {
        warn!(
            client = client_ua,
            "Detected non-compliant MCP client request - applying transformation"
        );

        match transform_request(&mut req) {
            Ok(_) => {
                info!(
                    from_method = %original_method,
                    to_method = %req.method(),
                    "Request transformed to spec-compliant format"
                );
            }
            Err(e) => {
                error!(error = %e, "Failed to transform request");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Proxy transformation error: {}", e),
                )
                    .into_response();
            }
        }
    } else {
        debug!(client = client_ua, "Request already spec-compliant, passing through");
    }

    // Forward to target server
    match forward_request(req, &config.target_url).await {
        Ok(response) => {
            debug!(status = ?response.status(), "Forwarded response to client");
            response.into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to forward request to server");
            (
                StatusCode::BAD_GATEWAY,
                format!("Proxy forwarding error: {}", e),
            )
                .into_response()
        }
    }
}

/// Check if request needs transformation
fn needs_transformation(req: &Request<Body>) -> bool {
    let method = req.method();
    let accept = req.headers().get("accept");

    // Need transform if:
    // 1. Method is GET (should be POST for MCP)
    let is_get = method == Method::GET;

    // 2. Accept header missing application/json
    let missing_json = accept
        .and_then(|v| v.to_str().ok())
        .map(|s| !s.contains("application/json"))
        .unwrap_or(true);

    is_get || missing_json
}

/// Transform request to be MCP spec-compliant
fn transform_request(req: &mut Request<Body>) -> Result<(), String> {
    // 1. Convert GET → POST
    let is_get = req.method() == Method::GET;
    if is_get {
        debug!("Transforming GET → POST");
        *req.method_mut() = Method::POST;
    }

    // 2. Fix Accept header
    let headers = req.headers_mut();
    if let Some(accept) = headers.get("accept").cloned() {
        let accept_str = accept.to_str().map_err(|e| e.to_string())?;

        if !accept_str.contains("application/json") {
            let new_accept = if accept_str.is_empty() {
                "application/json, text/event-stream".to_string()
            } else {
                format!("application/json, {}", accept_str)
            };

            debug!(
                from = accept_str,
                to = %new_accept,
                "Fixing Accept header"
            );

            headers.insert(
                "accept",
                HeaderValue::from_str(&new_accept).map_err(|e| e.to_string())?,
            );
        }
    } else {
        // No Accept header at all - add it
        debug!("Adding missing Accept header");
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
    }

    // 3. Ensure Content-Type for POST (check is_get to avoid re-borrowing req)
    if is_get && !headers.contains_key("content-type") {
        debug!("Adding Content-Type: application/json");
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );
    }

    Ok(())
}

/// Forward request to target server
async fn forward_request(
    req: Request<Body>,
    target_url: &str,
) -> Result<Response<Body>, String> {
    let client = reqwest::Client::new();

    let method = req.method().clone();
    let uri = req.uri();
    let path = uri.path();
    let query = uri.query().unwrap_or("");

    let url = if query.is_empty() {
        format!("{}{}", target_url, path)
    } else {
        format!("{}{}?{}", target_url, path, query)
    };

    debug!(method = %method, url = %url, "Forwarding to target");

    // Build request
    let mut builder = client.request(method, &url);

    // Copy headers (skip host and content-length as reqwest sets these)
    for (key, value) in req.headers() {
        let key_str = key.as_str();
        if key_str != "host" && key_str != "content-length" {
            builder = builder.header(key, value);
        }
    }

    // Get body
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    if !body_bytes.is_empty() {
        builder = builder.body(body_bytes.to_vec());
    }

    // Send request
    let response: reqwest::Response = builder
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Convert response
    let status = response.status();
    let mut resp_builder = Response::builder().status(status);

    // Copy response headers
    for (key, value) in response.headers() {
        resp_builder = resp_builder.header(key, value);
    }

    let body_bytes: bytes::Bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    resp_builder
        .body(Body::from(body_bytes.to_vec()))
        .map_err(|e| format!("Failed to build response: {}", e))
}
