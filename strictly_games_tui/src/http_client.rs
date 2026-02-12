//! HTTP-based player that connects to game server.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, instrument, warn};

/// Game board state from server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    /// The 9-cell board.
    pub board: Vec<Option<String>>,
    /// Current player's turn (X or O).
    pub current_player: String,
    /// Game status.
    pub status: String,
    /// Player X name.
    pub player_x: Option<String>,
    /// Player O name.
    pub player_o: Option<String>,
    /// Winner (if game over).
    pub winner: Option<String>,
}

/// HTTP client for game server.
#[derive(Debug, Clone)]
pub struct HttpGameClient {
    /// Base URL of game server.
    base_url: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Current session ID.
    pub session_id: String,
    /// Current player ID.
    pub player_id: String,
}

impl HttpGameClient {
    /// Creates a new HTTP game client by registering with server.
    #[instrument(skip_all, fields(base_url = %base_url, session_id = %session_id, name = %name))]
    pub async fn register(
        base_url: String,
        session_id: String,
        name: String,
    ) -> Result<Self> {
        info!(
            base_url = %base_url,
            session_id = %session_id,
            name = %name,
            "Registering with HTTP game server"
        );

        let client = reqwest::Client::new();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
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

        debug!(request = ?request, "Sending registration request");

        let response = client
            .post(&base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, base_url = %base_url, "Failed to send registration request");
                anyhow::anyhow!("HTTP request failed: {}", e)
            })?;

        let status = response.status();
        debug!(status = %status, "Received response");

        let text = response.text().await.map_err(|e| {
            error!(error = %e, "Failed to read response body");
            anyhow::anyhow!("Failed to read response: {}", e)
        })?;

        debug!(response = %text, "Response body");

        // Parse SSE format: "data: {...}"
        let json_str = text
            .strip_prefix("data: ")
            .unwrap_or(&text)
            .trim();

        let json: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            error!(error = %e, response = %text, "Failed to parse JSON response");
            anyhow::anyhow!("Invalid JSON response: {}", e)
        })?;

        debug!(json = ?json, "Parsed JSON response");

        // Check for JSON-RPC error
        if let Some(err) = json.get("error") {
            let error_msg = err["message"].as_str().unwrap_or("Unknown error");
            let error_code = err["code"].as_i64().unwrap_or(0);
            error!(
                error_message = error_msg,
                error_code = error_code,
                "Server returned error"
            );
            return Err(anyhow::anyhow!("Server error {}: {}", error_code, error_msg));
        }

        // Extract player_id from text content
        let content = json["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                error!(response = ?json, "Missing text content in response");
                anyhow::anyhow!("Missing text content in response")
            })?;

        // Parse "Player ID: game1_alice" from response
        let player_id = content
            .lines()
            .find(|line| line.starts_with("Player ID:"))
            .and_then(|line| line.split(": ").nth(1))
            .ok_or_else(|| {
                error!(content = %content, "Failed to extract player ID from response");
                anyhow::anyhow!("Failed to extract player ID from response")
            })?
            .to_string();

        info!(
            session_id = %session_id,
            player_id = %player_id,
            "Registered successfully with server"
        );

        Ok(Self {
            base_url,
            client,
            session_id,
            player_id,
        })
    }

    /// Makes a move at the given position.
    #[instrument(skip(self), fields(session_id = %self.session_id, player_id = %self.player_id))]
    pub async fn make_move(&self, position: usize) -> Result<()> {
        info!(position, "Sending move to server");

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "make_move",
                "arguments": {
                    "session_id": self.session_id,
                    "player_id": self.player_id,
                    "position": position
                }
            }
        });

        let response = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&request)
            .send()
            .await?;

        let text = response.text().await?;
        debug!(response = %text, "Move response");

        // Check for errors in response
        let json_str = text.strip_prefix("data: ").unwrap_or(&text).trim();
        let json: serde_json::Value = serde_json::from_str(json_str)?;

        if let Some(error) = json.get("error") {
            let error_msg = error["message"].as_str().unwrap_or("Unknown error");
            warn!(error = error_msg, "Move failed");
            return Err(anyhow::anyhow!("Move failed: {}", error_msg));
        }

        info!(position, "Move completed successfully");
        Ok(())
    }

    /// Gets the current board state.
    #[instrument(skip(self), fields(session_id = %self.session_id))]
    pub async fn get_board(&self) -> Result<BoardState> {
        debug!("Getting board state from server");

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "get_board",
                "arguments": {
                    "session_id": self.session_id
                }
            }
        });

        let response = self
            .client
            .post(&self.base_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&request)
            .send()
            .await?;

        let text = response.text().await?;

        let json_str = text.strip_prefix("data: ").unwrap_or(&text).trim();
        let json: serde_json::Value = serde_json::from_str(json_str)?;

        let content = json["result"]["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing text content in board response"))?;

        // Parse the text content
        let board_state = Self::parse_board_state(content)?;

        debug!(?board_state, "Parsed board state");
        Ok(board_state)
    }

    /// Parses board state from server text response.
    fn parse_board_state(text: &str) -> Result<BoardState> {
        let mut board = vec![None; 9];
        let mut current_player = String::new();
        let mut status = String::new();
        let mut player_x = None;
        let mut player_o = None;
        let mut winner = None;
        
        // Parse the visual board grid
        // Expected format:
        //  1 | 2 | 3
        // -----------
        //  4 | X | 6  
        // -----------
        //  7 | 8 | 9
        let mut in_board = false;
        let mut row = 0;
        
        for line in text.lines() {
            // Parse metadata
            if line.starts_with("Player X:") {
                player_x = Some(line.split(": ").nth(1).unwrap_or("").to_string());
            } else if line.starts_with("Player O:") {
                player_o = Some(line.split(": ").nth(1).unwrap_or("").to_string());
            } else if line.starts_with("Current player:") {
                current_player = line.split(": ").nth(1).unwrap_or("").to_string();
            } else if line.starts_with("Status:") {
                status = line.split(": ").nth(1).unwrap_or("").to_string();
            } else if line.starts_with("Winner:") {
                winner = Some(line.split(": ").nth(1).unwrap_or("").to_string());
            }
            
            // Parse board grid (lines with | separators)
            if line.contains('|') && !line.contains('-') {
                in_board = true;
                let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                
                if cells.len() == 3 {
                    for (col, cell) in cells.iter().enumerate() {
                        let pos = row * 3 + col;
                        if pos < 9 {
                            board[pos] = match *cell {
                                "X" => Some("X".to_string()),
                                "O" => Some("O".to_string()),
                                _ => None,
                            };
                        }
                    }
                    row += 1;
                }
            }
        }

        debug!(
            board = ?board,
            current_player = %current_player,
            status = %status,
            "Parsed board state"
        );

        Ok(BoardState {
            board,
            current_player,
            status,
            player_x,
            player_o,
            winner,
        })
    }
}
