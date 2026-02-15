//! Core domain types for tic-tac-toe.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Player in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit)]
pub enum Player {
    /// Player X (goes first).
    X,
    /// Player O (goes second).
    O,
}

impl Player {
    /// Returns the opponent player.
    #[instrument]
    pub fn opponent(self) -> Self {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

/// A square on the tic-tac-toe board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit)]
pub enum Square {
    /// Empty square.
    Empty,
    /// Square occupied by a player.
    Occupied(Player),
}

/// 3x3 tic-tac-toe board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Elicit)]
pub struct Board {
    /// Squares in row-major order (0-8).
    squares: [Square; 9],
}

impl Board {
    /// Creates a new empty board.
    #[instrument]
    pub fn new() -> Self {
        Self {
            squares: [Square::Empty; 9],
        }
    }

    /// Gets the square at the given position.
    #[instrument]
    pub fn get(&self, pos: super::Position) -> Square {
        self.squares[pos.to_index()]
    }

    /// Sets the square at the given position.
    #[instrument]
    pub fn set(&mut self, pos: super::Position, square: Square) {
        self.squares[pos.to_index()] = square;
    }

    /// Checks if a square is empty.
    #[instrument]
    pub fn is_empty(&self, pos: super::Position) -> bool {
        matches!(self.get(pos), Square::Empty)
    }

    /// Returns all squares as a slice.
    #[instrument]
    pub fn squares(&self) -> &[Square; 9] {
        &self.squares
    }

    /// Creates a board from a given array of squares (for Kani).
    #[cfg(kani)]
    pub fn from_squares(squares: [Square; 9]) -> Self {
        Self { squares }
    }

    /// Formats the board as a human-readable string.
    #[instrument]
    pub fn display(&self) -> String {
        use super::Position;
        let mut result = String::new();
        for row in 0..3 {
            for col in 0..3 {
                let pos = Position::from_index(row * 3 + col).unwrap();
                let symbol = match self.squares[pos.to_index()] {
                    Square::Empty => (pos.to_index() + 1).to_string(),
                    Square::Occupied(Player::X) => "X".to_string(),
                    Square::Occupied(Player::O) => "O".to_string(),
                };
                result.push_str(&symbol);
                if col < 2 {
                    result.push('|');
                }
            }
            if row < 2 {
                result.push_str("\n-+-+-\n");
            }
        }
        result
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}


