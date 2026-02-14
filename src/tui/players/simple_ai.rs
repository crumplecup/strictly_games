//! Simple AI player for testing (not using MCP).

use super::Player;
use anyhow::Result;
use crate::games::tictactoe::Game;
use tracing::debug;

/// Simple AI that picks first available square.
pub struct SimpleAI {
    name: String,
}

impl SimpleAI {
    /// Creates a new simple AI.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
        }
    }
}

#[async_trait::async_trait]
impl Player for SimpleAI {
    async fn get_move(&mut self, game: &Game) -> Result<usize> {
        debug!(ai = %self.name, "AI making move");
        
        // Add small delay to simulate thinking
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        let state = game.state();
        
        // Find first empty square
        for pos in 0..9 {
            if state.board().is_empty(pos) {
                debug!(ai = %self.name, position = pos, "AI chose position");
                return Ok(pos);
            }
        }
        
        anyhow::bail!("No valid moves available")
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}
