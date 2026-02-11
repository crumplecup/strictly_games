//! Game orchestration between players.

use crate::players::Player;
use anyhow::Result;
use strictly_games::games::tictactoe::{Game, GameStatus};
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
    MoveMade { player: String, position: usize },
    /// Game ended.
    GameOver { winner: Option<String> },
}

/// Orchestrates gameplay between two players.
pub struct Orchestrator {
    game: Game,
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
            game: Game::new(),
            player_x,
            player_o,
            event_tx,
        }
    }
    
    /// Runs the game loop.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting game orchestration");
        
        loop {
            let state = self.game.state();
            
            // Check if game is over
            match state.status() {
                GameStatus::Won(player) => {
                    let winner_name = if *player == strictly_games::games::tictactoe::Player::X {
                        self.player_x.name()
                    } else {
                        self.player_o.name()
                    };
                    
                    self.event_tx.send(GameEvent::GameOver {
                        winner: Some(winner_name.to_string()),
                    })?;
                    
                    return Ok(());
                }
                GameStatus::Draw => {
                    self.event_tx.send(GameEvent::GameOver { winner: None })?;
                    return Ok(());
                }
                GameStatus::InProgress => {
                    // Continue playing
                }
            }
            
            // Get current player
            let current_player = state.current_player();
            let is_x = current_player == strictly_games::games::tictactoe::Player::X;
            
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
            
            // Make the move
            self.game.make_move(position).map_err(|e| anyhow::anyhow!(e))?;
            
            // Notify UI
            self.event_tx.send(GameEvent::MoveMade {
                player: player_name,
                position,
            })?;
            
            self.event_tx.send(GameEvent::StateChanged(
                self.game.state().board().display(),
            ))?;
        }
    }
    
    /// Restarts the game.
    pub fn restart(&mut self) {
        self.game = Game::new();
    }
}
