//! Core domain types for tic-tac-toe.

use elicitation::Elicit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Player in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Elicit)]
#[cfg_attr(kani, derive(kani::Arbitrary, elicitation::KaniCompose))]
pub enum Player {
    /// Player X (goes first).
    X,
    /// Player O (goes second).
    O,
}

impl Player {
    /// Returns the opponent player.
    pub fn opponent(self) -> Self {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

/// A square on the tic-tac-toe board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Elicit)]
#[cfg_attr(kani, derive(kani::Arbitrary, elicitation::KaniCompose))]
pub enum Square {
    /// Empty square.
    Empty,
    /// Square occupied by a player.
    Occupied(Player),
}

/// 3x3 tic-tac-toe board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Elicit)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Board {
    /// Squares in row-major order (0-8).
    squares: [Square; 9],
}

impl Board {
    /// Creates a new empty board.
    pub fn new() -> Self {
        Self {
            squares: [Square::Empty; 9],
        }
    }

    /// Gets the square at the given position.
    pub fn get(&self, pos: super::Position) -> Square {
        self.squares[pos.to_index()]
    }

    /// Sets the square at the given position.
    pub fn set(&mut self, pos: super::Position, square: Square) {
        self.squares[pos.to_index()] = square;
    }

    /// Checks if a square is empty.
    pub fn is_empty(&self, pos: super::Position) -> bool {
        matches!(self.get(pos), Square::Empty)
    }

    /// Returns all squares as a slice.
    pub fn squares(&self) -> &[Square; 9] {
        &self.squares
    }

    /// Formats the board as a human-readable string.
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

/// Manual `KaniCompose` for `Board` — workaround for the missing `[T; N]: KaniCompose`
/// blanket impl.  Uses `std::array::from_fn` so each square is constructed symbolically.
/// TODO: remove once elicitation gains `impl<T: KaniCompose, const N: usize> KaniCompose for [T; N]`.
#[cfg(kani)]
impl elicitation::KaniCompose for Board {
    fn kani_depth0() -> Self {
        Self {
            squares: std::array::from_fn(|_| <Square as elicitation::KaniCompose>::kani_depth0()),
        }
    }

    fn kani_depth1() -> Self {
        Self {
            squares: std::array::from_fn(|_| <Square as elicitation::KaniCompose>::kani_depth1()),
        }
    }

    fn kani_depth2() -> Self {
        Self {
            squares: std::array::from_fn(|_| <Square as elicitation::KaniCompose>::kani_depth2()),
        }
    }
}
