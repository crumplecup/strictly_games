//! Stdio to HTTP Bridge for MCP
//!
//! Bridges Copilot CLI's working stdio transport to our HTTP game server.
//! 
//! ## Problem
//! Copilot CLI v0.0.407 HTTP/SSE transport is broken:
//! - Connects but never calls tools/list
//! - Tools never load
//! 
//! ## Solution
//! This bridge translates between transports:
//! - Copilot → stdio JSON-RPC → Bridge → HTTP POST → Server
//! - Server → HTTP response → Bridge → stdio JSON-RPC → Copilot
//!
//! ## Usage
//! ```bash
//! # Terminal 1: Start HTTP server
//! cargo run --bin server_http
//!
//! # Terminal 2: Configure Copilot to use bridge
//! # ~/.copilot/mcp-config.json:
//! {
//!   "mcpServers": {
//!     "strictly-games": {
//!       "command": "/path/to/stdio_http_bridge",
//!       "args": [],
//!       "env": {}
//!     }
//!   }
//! }
//!
//! # Terminal 3: Connect Copilot
//! copilot --additional-mcp-config @~/.copilot/mcp-config.json
//! ```

use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

/// HTTP server URL to forward requests to
const SERVER_URL: &str = "http://localhost:3000";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing to stderr (stdout is for JSON-RPC)
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_target(false)
        .init();

    info!("🌉 Starting Stdio-HTTP Bridge for MCP");
    info!("📡 Forwarding to: {}", SERVER_URL);
    info!("🔌 Reading JSON-RPC from stdin, writing to stdout");

    // Get session_id and player_id from environment for waker
    let session_id = std::env::var("GAME_SESSION_ID").ok();
    let player_name = std::env::var("AGENT_NAME").unwrap_or_else(|_| "Agent".to_string());

    let client = reqwest::Client::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Create channel for waker notifications
    let (notification_tx, mut notification_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Spawn waker task if we have a session_id
    if let Some(ref sid) = session_id {
        let waker_client = client.clone();
        let waker_session = sid.clone();
        let waker_name = player_name.clone();
        
        tokio::spawn(async move {
            waker_task(waker_client, waker_session, waker_name, notification_tx).await;
        });
        
        info!(session_id = %sid, "🔔 Waker task started - will notify when it's your turn");
    }
    
    // Spawn task to write notifications to stdout
    let stdout_clone = Arc::new(Mutex::new(stdout));
    let stdout_for_notifications = stdout_clone.clone();
    tokio::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            let mut out = stdout_for_notifications.lock().await;
            if writeln!(out, "{}", notification).is_ok() {
                out.flush().ok();
            }
        }
    });
    
    let mut stdout = stdout_clone;

    for line in stdin.lock().lines() {
        let line = line.context("Failed to read line from stdin")?;
        
        if line.trim().is_empty() {
            continue;
        }

        debug!(line = %line, "Received JSON-RPC from stdin");

        // Parse JSON-RPC request
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                error!(error = ?e, line = %line, "Failed to parse JSON-RPC");
                continue;
            }
        };

        // Forward to HTTP server
        match forward_to_http(&client, request.clone()).await {
            Ok(response) => {
                let response_str = serde_json::to_string(&response)
                    .context("Failed to serialize response")?;
                
                debug!(response = %response_str, "Sending response to stdout");
                
                let mut out = stdout.lock().await;
                writeln!(out, "{}", response_str)
                    .context("Failed to write response to stdout")?;
                out.flush()
                    .context("Failed to flush stdout")?;
            }
            Err(e) => {
                error!(error = ?e, "Failed to forward request to HTTP server");
                
                // Send error response back to client
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request.get("id"),
                    "error": {
                        "code": -32603,
                        "message": format!("Internal error: {}", e)
                    }
                });
                
                let mut out = stdout.lock().await;
                writeln!(out, "{}", serde_json::to_string(&error_response)?)
                    .context("Failed to write error response")?;
                out.flush()?;
            }
        }
    }

    info!("Stdin closed, bridge shutting down");
    Ok(())
}

#[instrument(skip(client, request), fields(method = %request.get("method").and_then(|v| v.as_str()).unwrap_or("unknown")))]
async fn forward_to_http(client: &reqwest::Client, mut request: Value) -> Result<Value> {
    // Auto-inject GAME_SESSION_ID from environment if not provided
    if let Some(params) = request.get_mut("params") {
        if let Some(params_obj) = params.as_object_mut() {
            // Only inject if session_id is missing and env var is set
            if !params_obj.contains_key("session_id") {
                if let Ok(session_id) = std::env::var("GAME_SESSION_ID") {
                    info!(session_id = %session_id, "Auto-injecting session_id from GAME_SESSION_ID env var");
                    params_obj.insert("session_id".to_string(), Value::String(session_id));
                }
            }
        }
    }

    let method = request.get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    
    info!(method = %method, "Forwarding request to HTTP server");

    // Send POST request to HTTP server
    let response = client
        .post(SERVER_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&request)
        .send()
        .await
        .context("Failed to send HTTP request")?;

    let status = response.status();
    debug!(status = %status, "Received HTTP response");

    if !status.is_success() {
        let error_text = response.text().await
            .unwrap_or_else(|_| "Unknown error".to_string());
        error!(status = %status, error = %error_text, "HTTP request failed");
        anyhow::bail!("HTTP request failed: {} - {}", status, error_text);
    }

    // Parse response body
    let body = response.text().await
        .context("Failed to read response body")?;

    debug!(body = %body, "Raw HTTP response body");

    // Strip SSE "data: " prefix if present
    let json_str = if body.starts_with("data: ") {
        body.strip_prefix("data: ")
            .unwrap_or(&body)
            .trim()
    } else {
        body.trim()
    };

    // Parse JSON response
    let json_response: Value = serde_json::from_str(json_str)
        .context("Failed to parse JSON response")?;

    info!(method = %method, "Successfully forwarded request");
    Ok(json_response)
}

/// Waker task that monitors game state and notifies agent when it's their turn.
#[instrument(skip(client, notification_tx))]
async fn waker_task(
    client: reqwest::Client,
    session_id: String,
    player_name: String,
    notification_tx: tokio::sync::mpsc::UnboundedSender<String>,
) {
    use tokio::time::{sleep, Duration};
    
    info!("Waker monitoring session for turn notifications");
    
    // Wait a bit for registration to complete
    sleep(Duration::from_secs(2)).await;
    
    let mut last_prompt_time = std::time::Instant::now();
    let prompt_cooldown = Duration::from_secs(10); // Don't spam prompts
    
    loop {
        sleep(Duration::from_millis(500)).await;
        
        // Get board state
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 999,
            "method": "tools/call",
            "params": {
                "name": "get_board",
                "arguments": {
                    "session_id": session_id
                }
            }
        });
        
        let Ok(response) = client
            .post(SERVER_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&request)
            .send()
            .await else {
                continue;
            };
        
        let Ok(text) = response.text().await else { continue; };
        
        let json_str = text.strip_prefix("data: ").unwrap_or(&text).trim();
        let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) else {
            error!("Failed to parse waker response JSON");
            continue;
        };
        
        // Extract board content
        let Some(content) = json["result"]["content"][0]["text"].as_str() else {
            error!("No text content in waker response");
            continue;
        };
        
        debug!(content = %content, "Waker checking game state");
        
        // Extract which player we are (from "Player O: Claude" line)
        let our_mark = if content.contains(&format!("Player O: {}", player_name)) {
            "O"
        } else if content.contains(&format!("Player X: {}", player_name)) {
            "X"
        } else {
            debug!("Could not determine our player mark, skipping turn check");
            continue;
        };
        
        // Check if it's our turn
        let is_our_turn = content.contains(&format!("Current player: {}", our_mark));
        let game_over = content.contains("Status: Won") || 
                       content.contains("Status: Draw");
        
        debug!(is_our_turn = is_our_turn, game_over = game_over, our_mark = our_mark, "Turn check result");
        
        if game_over {
            info!("Game over detected, waker stopping");
            break;
        }
        
        if is_our_turn && last_prompt_time.elapsed() > prompt_cooldown {
            // Send MCP notification to Copilot via channel
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/message",
                "params": {
                    "level": "info",
                    "message": format!("⏰ {}, it's your turn! Use make_move to play.", player_name)
                }
            });
            
            // Try writing a user-visible message to stderr (Copilot may display this)
            eprintln!("\n🎮 GAME UPDATE: It's your turn, {}! Check the board with get_board and make your move.\n", player_name);
            info!("Turn detected, wrote notification to stderr");
            
            last_prompt_time = std::time::Instant::now();
        }
    }
    
    info!("Waker task finished");
}
