//! HTTP Proxy for MCP Clients with Incomplete Accept Headers
//!
//! This proxy sits between MCP clients (copilot CLI, Claude, etc.) and the game server,
//! transforming non-compliant requests into spec-compliant ones.
//!
//! ## Problem
//! Some MCP clients (e.g., copilot CLI v0.0.407) send non-compliant requests:
//! - GET instead of POST (MCP requires POST for all requests)
//! - Empty body (should contain JSON-RPC initialize message)
//! - Missing `application/json` in Accept header
//!
//! ## Solution
//! Proxy transforms requests to be spec-compliant:
//! - GET → POST
//! - Adds default initialize body if empty
//! - `Accept: text/event-stream` → `Accept: application/json, text/event-stream`
//! - Adds `Content-Type: application/json`
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
    http::{Request, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use http_body_util::BodyExt;
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
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
#[tracing::instrument(skip(config, req), fields(method = %req.method(), uri = %req.uri(), client_ua))]
async fn proxy_handler(
    State(config): State<Arc<ProxyConfig>>,
    mut req: Request<Body>,
) -> Result<Response, StatusCode> {
    let original_method = req.method().clone();
    let original_uri = req.uri().clone();
    let client_ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();  // Clone to avoid borrow issues
    
    tracing::Span::current().record("client_ua", &client_ua);

    info!(
        method = %original_method,
        uri = %original_uri,
        user_agent = %client_ua,
        accept_header = ?req.headers().get("accept"),
        "Incoming request from client"
    );

    // Check if request needs transformation
    let needs_transform = needs_transformation(&req);

    if needs_transform {
        warn!(
            client = client_ua,
            original_method = %original_method,
            "Detected non-compliant MCP client request - applying transformation"
        );

        if let Err(e) = transform_request(&mut req).await {
            let err_msg = e.to_string();
            error!(
                error = %err_msg,
                client = %client_ua,
                original_method = %original_method,
                "Failed to transform request"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        info!(
            from_method = %original_method,
            to_method = %req.method(),
            client = %client_ua,
            "Transformed request to spec-compliant format"
        );
    } else {
        info!(
            client = %client_ua,
            method = %req.method(),
            "Request already spec-compliant, passing through"
        );
    }

    // Modify URI to point to target
    let path = req.uri().path();
    let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();
    let target_uri = format!("{}{}{}", config.target_url, path, query);
    
    info!(
        target_uri = %target_uri,
        method = %req.method(),
        "Forwarding request to backend"
    );
    
    *req.uri_mut() = target_uri
        .parse()
        .map_err(|e| {
            error!(error = ?e, target_uri = %target_uri, "Failed to parse target URI");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Create client and forward
    let client = Client::builder(TokioExecutor::new()).build_http();
    
    match client.request(req).await {
        Ok(resp) => {
            let status = resp.status();
            let content_type = resp.headers().get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown");
            
            info!(
                status = %status,
                content_type = content_type,
                "Received response from backend"
            );
            
            // Convert hyper response body to axum body
            let (mut parts, body) = resp.into_parts();
            
            debug!("Starting body collection");
            let collected = body
                .collect()
                .await
                .map_err(|e| {
                    error!(
                        error = ?e,
                        status = %status,
                        "Failed to collect response body from backend"
                    );
                    StatusCode::BAD_GATEWAY
                })?;
            let body_bytes = collected.to_bytes();
            let body_len = body_bytes.len();
            let body_preview = String::from_utf8_lossy(&body_bytes[..body_bytes.len().min(200)]).to_string();
            
            info!(
                bytes_len = body_len,
                body_preview = %body_preview,
                "Body collected from backend"
            );
            
            // Remove Transfer-Encoding: chunked since we've collected the full body
            let had_transfer_encoding = parts.headers.remove("transfer-encoding").is_some();
            debug!(had_transfer_encoding, "Removed transfer-encoding header");
            
            // Set Content-Length since we have the full body now
            parts.headers.insert(
                "content-length",
                body_len.to_string().parse().unwrap(),
            );
            debug!(content_length = body_len, "Set content-length header");
            
            let response = Response::from_parts(parts, Body::from(body_bytes));
            
            info!(
                status = %status,
                content_length = body_len,
                "Forwarding response to client"
            );
            Ok(response)
        }
        Err(e) => {
            error!(
                error = %e,
                target = %config.target_url,
                "Failed to forward request to backend"
            );
            Err(StatusCode::BAD_GATEWAY)
        }
    }
}

/// Check if request needs transformation
#[tracing::instrument(skip(req), fields(method = %req.method()))]
fn needs_transformation(req: &Request<Body>) -> bool {
    let accept = req.headers().get("accept");

    // Only need transform if Accept header missing application/json
    let missing_json = accept
        .and_then(|v| v.to_str().ok())
        .map(|s| !s.contains("application/json"))
        .unwrap_or(true);
    
    debug!(
        accept_header = ?accept.and_then(|v| v.to_str().ok()),
        missing_json = missing_json,
        "Checked if request needs transformation"
    );
    
    missing_json
}

/// Transform request to be MCP spec-compliant
#[tracing::instrument(skip(req), fields(method = %req.method()))]
async fn transform_request(req: &mut Request<Body>) -> Result<(), String> {
    use axum::http::{HeaderValue, Method};
    use http_body_util::BodyExt;
    
    // 1. Convert GET → POST (MCP requires POST for all requests)
    let is_get = req.method() == Method::GET;
    if is_get {
        info!("Converting GET → POST (MCP requires POST for all operations)");
        *req.method_mut() = Method::POST;
        
        // Add default initialization body if empty
        let body_bytes = req.body_mut()
            .collect()
            .await
            .map_err(|e| {
                let err = format!("Failed to read body: {}", e);
                error!(error = %e, "Body read failed during transformation");
                err
            })?
            .to_bytes();
        
        let body_len = body_bytes.len();
        debug!(original_body_len = body_len, "Read original request body");
            
        if body_bytes.is_empty() {
            let init_body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"copilot-proxy","version":"0.1.0"}}}"#;
            info!(
                body_len = init_body.len(),
                "Adding default initialize message body"
            );
            *req.body_mut() = Body::from(init_body.to_string());
        } else {
            info!(body_len, "Preserving existing request body");
            *req.body_mut() = Body::from(body_bytes);
        }
    }
    
    // 2. Fix Accept header
    let headers = req.headers_mut();
    if let Some(accept) = headers.get("accept").cloned() {
        let accept_str = accept.to_str().map_err(|e| {
            error!(error = ?e, "Failed to parse Accept header");
            e.to_string()
        })?;

        if !accept_str.contains("application/json") {
            let new_accept = if accept_str.is_empty() {
                "application/json, text/event-stream".to_string()
            } else {
                format!("application/json, {}", accept_str)
            };

            info!(
                from = accept_str,
                to = %new_accept,
                "Fixed Accept header to include application/json"
            );

            headers.insert(
                "accept",
                HeaderValue::from_str(&new_accept).map_err(|e| {
                    error!(error = ?e, "Failed to set Accept header");
                    e.to_string()
                })?,
            );
        } else {
            debug!(accept = accept_str, "Accept header already compliant");
        }
    } else {
        // No Accept header at all - add it
        info!("Adding missing Accept header with both application/json and text/event-stream");
        headers.insert(
            "accept",
            HeaderValue::from_static("application/json, text/event-stream"),
        );
    }
    
    // 3. Ensure Content-Type for POST
    if !headers.contains_key("content-type") {
        info!("Adding Content-Type: application/json for POST request");
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );
    } else {
        debug!(
            content_type = ?headers.get("content-type"),
            "Content-Type already present"
        );
    }

    info!("Request transformation complete");
    Ok(())
}
