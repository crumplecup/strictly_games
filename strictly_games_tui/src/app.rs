//! Application state and logic.

use strictly_games::games::tictactoe::Game;
use tracing::debug;

use crate::orchestrator::GameEvent;

/// Main application state.
pub struct App {
    game: Game,
    status_message: String,
    current_player_name: Option<String>,
}

impl App {
    /// Creates a new application.
    pub fn new() -> Self {
        Self {
            game: Game::new(),
            status_message: "Waiting for game to start...".to_string(),
            current_player_name: None,
        }
    }

    /// Gets the current game.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Gets the current status message.
    pub fn status_message(&self) -> &str {
        &self.status_message
    }

    /// Handles a game event from the orchestrator.
    pub fn handle_event(&mut self, event: GameEvent) {
        debug!(?event, "Handling game event");

        match event {
            GameEvent::StateChanged(message) => {
                self.status_message = message;
            }
            GameEvent::AgentThinking => {
                if let Some(name) = &self.current_player_name {
                    self.status_message = format!("{} is thinking...", name);
                } else {
                    self.status_message = "AI is thinking...".to_string();
                }
            }
            GameEvent::MoveMade { player, position } => {
                // Update our game state
                if let Err(e) = self.game.make_move(position) {
                    self.status_message = format!("Move error: {}", e);
                } else {
                    debug!(player = %player, position, "Move applied to UI state");
                    self.status_message = format!("{} played position {}", player, position + 1);
                }
            }
            GameEvent::GameOver { winner } => {
                self.status_message = match winner {
                    Some(player) => {
                        format!("{} wins! Press 'r' to restart or 'q' to quit.", player)
                    }
                    None => {
                        "Game ended in a draw! Press 'r' to restart or 'q' to quit.".to_string()
                    }
                };
            }
        }
    }

    /// Restarts the game.
    pub fn restart(&mut self) {
        debug!("Restarting game");
        self.game = Game::new();
        self.status_message = "Game restarted. Player X's turn.".to_string();
        self.current_player_name = None;
    }
}
