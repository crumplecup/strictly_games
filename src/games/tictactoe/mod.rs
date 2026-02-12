mod types;
mod rules;

pub use types::{Board, Player, Square, GameState, GameStatus};
pub use rules::Game;

/// Alias for clarity in session management.
pub type Mark = Player;
