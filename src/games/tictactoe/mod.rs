//! Tic-tac-toe game implementation.

pub mod contracts;
pub mod types;
mod rules;

pub use contracts::{execute_move, validate_move};
pub use rules::Game;
pub use types::{GameStatus, Move, Player};

/// Alias for clarity in session management.
pub type Mark = Player;
