//! Application state and logic.

use anyhow::Result;
use strictly_games::games::tictactoe::{Game, GameStatus};
use tracing::debug;

/// Main application state.
pub struct App {
    game: Game,
    status_message: String,
}

impl App {
    /// Creates a new application.
    pub fn new() -> Self {
        Self {
            game: Game::new(),
            status_message: "Player X's turn. Press 1-9 to make a move.".to_string(),
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

    /// Makes a move at the given position.
    pub fn make_move(&mut self, position: usize) -> Result<()> {
        debug!(position, "Making move");

        match self.game.make_move(position) {
            Ok(()) => {
                let state = self.game.state();
                self.status_message = match state.status() {
                    GameStatus::InProgress => {
                        format!("Player {:?}'s turn", state.current_player())
                    }
                    GameStatus::Won(player) => {
                        format!("Player {:?} wins! Press 'r' to restart or 'q' to quit.", player)
                    }
                    GameStatus::Draw => {
                        "Game ended in a draw! Press 'r' to restart or 'q' to quit.".to_string()
                    }
                };
                Ok(())
            }
            Err(e) => {
                self.status_message = format!("Invalid move: {}. Try again.", e);
                Ok(())
            }
        }
    }

    /// Restarts the game.
    pub fn restart(&mut self) {
        debug!("Restarting game");
        self.game = Game::new();
        self.status_message = "Player X's turn. Press 1-9 to make a move.".to_string();
    }
}
