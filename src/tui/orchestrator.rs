//! Game orchestration between players.

use super::players::Player;
use anyhow::Result;
use crate::games::tictactoe::{AnyGame, Position, Player as Mark};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Messages sent from orchestrator to UI.
#[derive(Debug, Clone)]
pub enum GameEvent {
    /// Game state updated.
    StateChanged(String),
    /// Agent is thinking.
    AgentThinking,
    /// Move was made.
    MoveMade { player: String, position: Position },
    /// Game ended.
    GameOver { winner: Option<String> },
}

/// Orchestrates gameplay between two players.
pub struct Orchestrator {
    game: AnyGame,
    player_x: Box<dyn Player>,
    player_o: Box<dyn Player>,
    event_tx: mpsc::UnboundedSender<GameEvent>,
}

impl Orchestrator {
    /// Creates a new orchestrator.
    pub fn new(
        player_x: Box<dyn Player>,
        player_o: Box<dyn Player>,
        event_tx: mpsc::UnboundedSender<GameEvent>,
    ) -> Self {
        Self {
            game: crate::games::tictactoe::Game::new().into(),
            player_x,
            player_o,
            event_tx,
        }
    }
    
    /// Runs the game loop.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting game orchestration");
        
        loop {
            // Check if game is over
            if self.game.is_over() {
                if let Some(winner) = self.game.winner() {
                    let winner_name = if winner == Mark::X {
                        self.player_x.name()
                    } else {
                        self.player_o.name()
                    };
                    
                    self.event_tx.send(GameEvent::GameOver {
                        winner: Some(winner_name.to_string()),
                    })?;
                    
                    return Ok(());
                } else {
                    self.event_tx.send(GameEvent::GameOver { winner: None })?;
                    return Ok(());
                }
            }
            
            // Get current player
            let current_player = self.game.to_move()
                .expect("Game not over but no current player");
            let is_x = current_player == Mark::X;
            
            // Get player name first (immutable borrow)
            let player_name = if is_x {
                self.player_x.name().to_string()
            } else {
                self.player_o.name().to_string()
            };
            
            // Then get mutable reference
            let player = if is_x {
                &mut self.player_x
            } else {
                &mut self.player_o
            };
            
            // Notify UI if agent is thinking
            if player_name.contains("Agent") {
                self.event_tx.send(GameEvent::AgentThinking)?;
            }
            
            // Get move from player
            debug!(player = %player_name, "Waiting for move");
            let position = player.get_move(&self.game).await?;
            
            // Make the move (AnyGame handles typestate transitions)
            let old_game = std::mem::replace(&mut self.game, crate::games::tictactoe::Game::new().into());
            self.game = old_game.place(position)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            
            // Notify UI
            self.event_tx.send(GameEvent::MoveMade {
                player: player_name,
                position,
            })?;
            
            self.event_tx.send(GameEvent::StateChanged(
                self.game.board().display(),
            ))?;
        }
    }
    
    /// Restarts the game.
    pub fn restart(&mut self) {
        self.game = crate::games::tictactoe::Game::new().into();
    }
}
