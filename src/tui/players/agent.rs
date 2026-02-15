//! AI agent player that prompts agent via MCP sampling.

use super::Player;
use anyhow::Result;
use rmcp::model::{CreateMessageRequestParams, Role, SamplingMessage};
use rmcp::service::{Peer, RoleServer};
use std::sync::Arc;
use crate::games::tictactoe::{AnyGame, Position};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Agent player that prompts an MCP client for moves.
/// 
/// Uses MCP's sampling API to send prompts to the agent,
/// then waits for the agent to call make_move via MCP tools.
pub struct AgentPlayer {
    name: String,
    peer: Option<Arc<Peer<RoleServer>>>,
    move_rx: mpsc::UnboundedReceiver<Position>,
}

impl AgentPlayer {
    /// Creates a new agent player with optional MCP peer for prompting.
    pub fn new(
        name: impl Into<String>,
        move_rx: mpsc::UnboundedReceiver<Position>,
        peer: Option<Arc<Peer<RoleServer>>>,
    ) -> Self {
        let name = name.into();
        info!(agent = %name, has_peer = peer.is_some(), "Creating agent player");

        Self {
            name,
            peer,
            move_rx,
        }
    }
}

#[async_trait::async_trait]
impl Player for AgentPlayer {
    async fn get_move(&mut self, game: &AnyGame) -> Result<Position> {
        debug!(agent = %self.name, "Agent's turn");

        // If we have a peer, send a prompt to the agent
        if let Some(peer) = &self.peer {
            let board = game.board().display();
            let current_player = game.to_move()
                .ok_or_else(|| anyhow::anyhow!("Game is over"))?;

            let prompt = format!(
                "It's your turn! You are playing as {:?}.\n\n\
                Current board:\n{}\n\n\
                Please call the make_move tool with a position (0-8) for your next move.\n\
                Positions are numbered left-to-right, top-to-bottom (0=top-left, 8=bottom-right).",
                current_player, board
            );

            info!(agent = %self.name, "Sending prompt to agent");

            let params = CreateMessageRequestParams {
                messages: vec![SamplingMessage {
                    role: Role::User,
                    content: rmcp::model::SamplingContent::Single(
                        rmcp::model::SamplingMessageContent::Text(
                            rmcp::model::RawTextContent {
                                text: prompt,
                                meta: None,
                            }
                        )
                    ),
                    meta: None,
                }],
                model_preferences: None,
                system_prompt: Some(
                    "You are playing tic-tac-toe. Use the make_move tool to make your moves.".to_string()
                ),
                include_context: None,
                temperature: None,
                max_tokens: 100,
                stop_sequences: None,
                metadata: None,
                tool_choice: None,
                tools: None,
                meta: None,
                task: None,
            };

            match peer.create_message(params).await {
                Ok(_response) => {
                    debug!(agent = %self.name, "Agent responded to prompt");
                    // Response might contain the tool call, but we still wait for channel
                }
                Err(e) => {
                    warn!(agent = %self.name, error = %e, "Failed to send prompt to agent");
                }
            }
        } else {
            info!(agent = %self.name, "No peer connection - waiting for manual move");
        }

        // Wait for agent to call make_move tool (sent via channel)
        // Use timeout to allow user to quit/restart if agent is stuck
        let timeout_duration = std::time::Duration::from_secs(60);
        
        match tokio::time::timeout(timeout_duration, self.move_rx.recv()).await {
            Ok(Some(position)) => {
                debug!(agent = %self.name, position = ?position, "Received move from agent");
                Ok(position)
            }
            Ok(None) => {
                anyhow::bail!("Agent disconnected (MCP channel closed)")
            }
            Err(_) => {
                warn!(agent = %self.name, "Agent move timed out after 60s");
                anyhow::bail!("Agent did not respond within 60 seconds")
            }
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

