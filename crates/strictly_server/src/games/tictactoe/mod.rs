//! Tic-tac-toe game implementation.
//!
//! This module provides application-specific wrappers around the pure game logic
//! from strictly_tictactoe.

// Re-export core types from strictly_tictactoe
pub use strictly_tictactoe::{Board, Player, Position, Square, rules};

// Application-specific wrappers
pub mod action;
pub mod contracts;
pub mod outcome;
pub mod typestate;
pub mod wrapper;

// Re-export application types
pub use action::{Move, MoveError};
pub use outcome::Outcome;
pub use typestate::{GameFinished, GameInProgress, GameResult, GameSetup};
pub use wrapper::AnyGame;

/// Alias for clarity in session management.
pub type Mark = Player;

/// Compatibility alias for GameSetup
pub type Game = GameSetup;
