//! AI agent player that uses elicitation to get moves.

use super::Player;
use anyhow::{Context, Result};
use elicitation::{ElicitClient, Elicitation};
use rmcp::service::{Peer, RoleClient};
use std::sync::Arc;
use strictly_games::games::tictactoe::Game;
use tracing::{debug, info};

/// Agent player that uses elicitation to ask an agent for moves.
pub struct AgentPlayer {
    name: String,
    client: ElicitClient,
}

impl AgentPlayer {
    /// Creates a new agent player with an MCP client.
    pub fn new(name: impl Into<String>, peer: Arc<Peer<RoleClient>>) -> Self {
        let name = name.into();
        info!(agent = %name, "Creating agent player");

        let client = ElicitClient::new(peer);

        Self { name, client }
    }
}

#[async_trait::async_trait]
impl Player for AgentPlayer {
    async fn get_move(&mut self, game: &Game) -> Result<usize> {
        debug!(agent = %self.name, "Eliciting move from agent");

        // Show the agent the current board
        let board = game.state().board().display();
        let current_player = game.state().current_player();
        
        info!(
            agent = %self.name,
            board = %board,
            "Asking agent for move"
        );

        // TODO: Need to provide context to the agent somehow
        // For now, elicit a simple number (1-9)
        let position: u8 = u8::elicit(&self.client)
            .await
            .context("Failed to elicit move from agent")?;

        // Validate range
        if position < 1 || position > 9 {
            anyhow::bail!("Agent returned invalid position: {}", position);
        }

        // Convert 1-9 to 0-8
        let pos = (position - 1) as usize;
        debug!(agent = %self.name, position = pos, "Agent chose position");
        
        Ok(pos)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

