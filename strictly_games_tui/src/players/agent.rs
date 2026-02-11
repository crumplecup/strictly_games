//! AI agent player that uses MCP to make moves.

use super::Player;
use anyhow::Result;
use elicitation::ElicitClient;
use serde_json::Value;
use std::sync::Arc;
use strictly_games::games::tictactoe::Game;
use tracing::{debug, info};

/// AI agent player using MCP.
pub struct AgentPlayer {
    name: String,
    client: Arc<ElicitClient>,
}

impl AgentPlayer {
    /// Creates a new agent player.
    pub fn new(name: impl Into<String>, client: Arc<ElicitClient>) -> Self {
        Self {
            name: name.into(),
            client,
        }
    }
}

#[async_trait::async_trait]
impl Player for AgentPlayer {
    async fn get_move(&mut self, game: &Game) -> Result<usize> {
        info!(agent = %self.name, "Agent making move");
        
        let state = game.state();
        let board_display = state.board().display();
        
        // Call the make_move tool via MCP
        let tool_name = "make_move";
        let args = serde_json::json!({
            "board": board_display,
            "current_player": format!("{:?}", state.current_player()),
        });
        
        debug!(tool = tool_name, args = ?args, "Calling agent tool");
        
        // TODO: This is a placeholder - we need to actually call the MCP tool
        // For now, let's make a simple AI move (first empty square)
        for pos in 0..9 {
            if state.board().is_empty(pos) {
                return Ok(pos);
            }
        }
        
        anyhow::bail!("No valid moves available")
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}
