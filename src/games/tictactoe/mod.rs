//! Tic-tac-toe game implementation.

pub mod types;
mod rules;

pub use types::{Player, GameStatus, Move};
pub use rules::Game;

/// Alias for clarity in session management.
pub type Mark = Player;
