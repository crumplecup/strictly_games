//! Game logic and rules for tic-tac-toe.

use super::types::{GameState, GameStatus, Player, Square};
use tracing::instrument;

/// Tic-tac-toe game engine.
#[derive(Debug, Clone)]
pub struct Game {
    state: GameState,
}

impl Game {
    /// Creates a new game.
    #[instrument]
    pub fn new() -> Self {
        Self {
            state: GameState::new(),
        }
    }

    /// Returns the current game state.
    #[instrument]
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// Returns mutable reference to game state.
    #[instrument]
    pub fn state_mut(&mut self) -> &mut GameState {
        &mut self.state
    }

    /// Makes a move at the given position.
    #[instrument]
    pub fn make_move(&mut self, pos: super::Position) -> Result<(), String> {
        // Check if game is over
        if self.state.status() != &GameStatus::InProgress {
            return Err("Game is already over".to_string());
        }

        // Check if square is empty
        if !self.state.board().is_empty(pos) {
            return Err("Square is already occupied".to_string());
        }

        // Apply the move
        let player = self.state.current_player();
        self.state.apply_move(pos, player);

        // Check for win or draw
        self.update_status();

        Ok(())
    }

    /// Updates game status after a move.
    fn update_status(&mut self) {
        if let Some(winner) = self.check_winner() {
            self.state.set_status(GameStatus::Won(winner));
        } else if self.is_board_full() {
            self.state.set_status(GameStatus::Draw);
        }
    }

    /// Checks if there's a winner.
    fn check_winner(&self) -> Option<Player> {
        use super::Position;
        let board = self.state.board();
        
        // Winning combinations
        const LINES: [[Position; 3]; 8] = [
            // Rows
            [Position::TopLeft, Position::TopCenter, Position::TopRight],
            [Position::MiddleLeft, Position::Center, Position::MiddleRight],
            [Position::BottomLeft, Position::BottomCenter, Position::BottomRight],
            // Columns
            [Position::TopLeft, Position::MiddleLeft, Position::BottomLeft],
            [Position::TopCenter, Position::Center, Position::BottomCenter],
            [Position::TopRight, Position::MiddleRight, Position::BottomRight],
            // Diagonals
            [Position::TopLeft, Position::Center, Position::BottomRight],
            [Position::TopRight, Position::Center, Position::BottomLeft],
        ];

        for line in &LINES {
            let [a, b, c] = *line;
            let occ = board.get(a);

            if occ != Square::Empty
                && occ == board.get(b)
                && occ == board.get(c)
            {
                return match occ {
                    Square::Occupied(p) => Some(p),
                    Square::Empty => None,
                };
            }
        }

        None
    }

    /// Checks if the board is full.
    fn is_board_full(&self) -> bool {
        self.state.board().squares().iter().all(|&s| s != Square::Empty)
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
