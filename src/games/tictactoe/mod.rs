//! Tic-tac-toe game implementation.

pub mod contracts;
pub mod position;
pub mod types;
mod rules;

pub use contracts::{execute_move, validate_move};
pub use position::Position;
pub use rules::Game;
pub use types::{Board, GameState, GameStatus, Move, Player, Square};

/// Alias for clarity in session management.
pub type Mark = Player;
