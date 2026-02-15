//! Type-safe HTTP client using REST API.

use anyhow::{Context, Result};
use crate::games::tictactoe::{AnyGame, Position};
use tracing::{debug, info, instrument};

/// Type-safe HTTP game client.
#[derive(Debug, Clone)]
pub struct RestGameClient {
    base_url: String,
    client: reqwest::Client,
    pub session_id: String,
    pub player_id: String,
    pub last_error: Option<String>,  // Track last error for display
    mcp_session_id: String,  // For MCP tool calls
}

impl RestGameClient {
    /// Creates a new REST client by registering with the server via MCP.
    #[instrument(skip_all, fields(base_url = %base_url, session_id = %session_id, name = %name))]
    pub async fn register(
        base_url: String,
        session_id: String,
        name: String,
    ) -> Result<Self> {
        info!("Registering with server");
        
        let client = reqwest::Client::new();
        
        // Register via MCP (keep this for player setup)
        let (player_id, mcp_session_id) = Self::mcp_register(&client, &base_url, &session_id, &name).await?;
        
        Ok(Self {
            base_url,
            client,
            session_id,
            player_id,
            last_error: None,
            mcp_session_id,
        })
    }
    
    /// MCP registration (creates player association).
    async fn mcp_register(
        client: &reqwest::Client,
        base_url: &str,
        session_id: &str,
        name: &str,
    ) -> Result<(String, String)> {
        // Initialize MCP session
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "strictly-games-tui",
                    "version": "0.1.0"
                }
            }
        });
        
        let response = client
            .post(&format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&init_req)
            .send()
            .await?;
            
        let mcp_session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
            .context("No MCP session ID in response headers")?
            .to_string();
        
        debug!(mcp_session_id = %mcp_session_id, "MCP session initialized");
        
        // Send initialized notification
        let init_notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        
        client
            .post(&format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &mcp_session_id)
            .json(&init_notif)
            .send()
            .await?;
        
        // Register player
        let register_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "register_player",
                "arguments": {
                    "session_id": session_id,
                    "name": name,
                    "type": "human"
                }
            }
        });
        
        let response = client
            .post(&format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &mcp_session_id)
            .json(&register_req)
            .send()
            .await?;
        
        let response_text = response.text().await?;
        debug!(response = %response_text, "Register player response");
        
        // Check for errors in response
        if response_text.contains("\"error\"") {
            return Err(anyhow::anyhow!("Registration failed: {}", response_text));
        }
        
        let player_id = format!("{}_{}", session_id, name.to_lowercase());
        info!(player_id = %player_id, "Registered successfully");
        
        Ok((player_id, mcp_session_id))
    }
    
    /// Gets the current game state (type-safe!).
    #[instrument(skip(self))]
    pub async fn get_game(&self) -> Result<AnyGame> {
        debug!("Getting game state via REST");
        
        let url = format!("{}/api/sessions/{}/game", self.base_url, self.session_id);
        let game: AnyGame = self.client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        debug!(is_over = game.is_over(), "Got game state");
        Ok(game)
    }
    
    /// Makes a move via MCP tool.
    #[instrument(skip(self), fields(position = ?position))]
    pub async fn make_move(&mut self, position: Position) -> Result<()> {
        info!("Making move");
        self.last_error = None;  // Clear previous error
        
        // Serialize Position properly using serde
        let position_value = serde_json::to_value(&position)?;
        debug!(position_json = %position_value, "Serialized position");
        
        // Use MCP tool for making moves (triggers elicitation)
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "make_move",
                "arguments": {
                    "session_id": self.session_id,
                    "player_id": self.player_id,
                    "position": position_value
                }
            }
        });
        
        debug!(request = %request, "Sending MCP tool call");
        
        let response = self.client
            .post(&format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.mcp_session_id)
            .json(&request)
            .send()
            .await?;
        
        let status = response.status();
        let body = response.text().await?;
        debug!(status = %status, body = %body, "Got MCP response");
        
        // Check for error in JSON-RPC response
        if body.contains("\"error\"") {
            // Parse error message
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(error_msg) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                    self.last_error = Some(error_msg.to_string());
                    anyhow::bail!("Move failed: {}", error_msg);
                }
            }
        }
        
        if !status.is_success() {
            self.last_error = Some(format!("HTTP {}", status));
            anyhow::bail!("Move failed: {} - {}", status, body);
        }
        
        Ok(())
    }
    
    /// Restarts current game (keeps players registered).
    #[instrument(skip(self))]
    pub async fn restart_game(&mut self) -> Result<()> {
        info!("Restarting game");
        self.last_error = None;
        
        let response = self.client
            .post(&format!("{}/api/sessions/{}/restart", self.base_url, self.session_id))
            .send()
            .await?;
        
        if !response.status().is_success() {
            self.last_error = Some("Restart failed".to_string());
            anyhow::bail!("Restart failed: {}", response.status());
        }
        
        Ok(())
    }
}
