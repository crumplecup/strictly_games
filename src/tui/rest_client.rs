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
        let player_id = Self::mcp_register(&client, &base_url, &session_id, &name).await?;
        
        Ok(Self {
            base_url,
            client,
            session_id,
            player_id,
        })
    }
    
    /// MCP registration (creates player association).
    async fn mcp_register(
        client: &reqwest::Client,
        base_url: &str,
        session_id: &str,
        name: &str,
    ) -> Result<String> {
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
            .json(&init_req)
            .send()
            .await?;
            
        let mcp_session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
            .context("No MCP session ID")?
            .to_string();
        
        // Register player
        let register_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "register_player",
                "arguments": {
                    "session_id": session_id,
                    "name": name
                }
            }
        });
        
        let _response = client
            .post(&format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("mcp-session-id", &mcp_session_id)
            .json(&register_req)
            .send()
            .await?;
        
        let player_id = format!("{}_{}", session_id, name.to_lowercase());
        info!(player_id = %player_id, "Registered successfully");
        
        Ok(player_id)
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
    pub async fn make_move(&self, position: Position) -> Result<()> {
        info!("Making move");
        
        // Serialize Position properly using serde
        let position_value = serde_json::to_value(&position)?;
        
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
        
        let response = self.client
            .post(&format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            anyhow::bail!("Move failed: {}", response.status());
        }
        
        Ok(())
    }
    
    /// Starts a new game via MCP tool.
    #[instrument(skip(self))]
    pub async fn start_game(&self) -> Result<()> {
        info!("Starting new game");
        
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "start_game",
                "arguments": {
                    "session_id": self.session_id
                }
            }
        });
        
        let response = self.client
            .post(&format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;
        
        if !response.status().is_success() {
            anyhow::bail!("Start game failed: {}", response.status());
        }
        
        Ok(())
    }
}
