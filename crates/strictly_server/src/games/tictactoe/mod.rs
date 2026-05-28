//! Tic-tac-toe game implementation.
//!
//! Pure game logic lives in `strictly_tictactoe`.
//! This module provides the server-side factory and wrapper.

// Re-export all pure game types from strictly_tictactoe
pub use strictly_tictactoe::{
    Board, GameFinished, GameInProgress, GameResult, GameSetup, Move, MoveError, Outcome, Player,
    PlayerTurn, Position, Square, SquareEmpty,
};

// Server-specific modules
pub mod contracts;
pub mod display;
pub mod factory;
mod render_verify;
pub mod wrapper;

pub use contracts::BoardColumnsAligned;
pub use factory::{TttGameContext, register_await_turn_tool, register_move_tools};
pub use wrapper::AnyGame;

/// Alias for clarity in session management.
pub type Mark = Player;

/// Compatibility alias for GameSetup
pub type Game = GameSetup;
