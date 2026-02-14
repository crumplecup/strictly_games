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

    /// Makes a move at the given position (0-8).
    #[instrument]
    pub fn make_move(&mut self, pos: usize) -> Result<(), String> {
        // Check if game is over
        if self.state.status() != &GameStatus::InProgress {
            return Err("Game is already over".to_string());
        }

        // Check position bounds
        if pos >= 9 {
            return Err("Position out of bounds (must be 0-8)".to_string());
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
        let board = self.state.board();
        
        // Winning combinations
        const LINES: [[usize; 3]; 8] = [
            [0, 1, 2], [3, 4, 5], [6, 7, 8], // Rows
            [0, 3, 6], [1, 4, 7], [2, 5, 8], // Columns
            [0, 4, 8], [2, 4, 6],             // Diagonals
        ];

        for line in &LINES {
            let squares: Vec<_> = line.iter().filter_map(|&i| board.get(i)).collect();
            if squares.len() == 3
                && let [Square::Occupied(p1), Square::Occupied(p2), Square::Occupied(p3)] = squares.as_slice()
                    && p1 == p2 && p2 == p3 {
                        return Some(*p1);
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
