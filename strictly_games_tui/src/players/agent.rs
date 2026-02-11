//! AI agent player that receives moves via MCP server.

use super::Player;
use anyhow::{Context, Result};
use strictly_games::games::tictactoe::Game;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Agent player that waits for moves from MCP server.
/// 
/// When the agent calls make_move via MCP, the server sends
/// the move through this channel.
pub struct AgentPlayer {
    name: String,
    move_rx: mpsc::UnboundedReceiver<usize>,
}

impl AgentPlayer {
    /// Creates a new agent player with a move receiver channel.
    pub fn new(name: impl Into<String>, move_rx: mpsc::UnboundedReceiver<usize>) -> Self {
        let name = name.into();
        info!(agent = %name, "Creating agent player");

        Self { name, move_rx }
    }
}

#[async_trait::async_trait]
impl Player for AgentPlayer {
    async fn get_move(&mut self, _game: &Game) -> Result<usize> {
        debug!(agent = %self.name, "Waiting for agent move via MCP");

        // Wait for agent to call make_move tool
        let position = self.move_rx.recv()
            .await
            .context("Agent disconnected (MCP channel closed)")?;

        debug!(agent = %self.name, position, "Received move from agent");
        Ok(position)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

