//! Strictly Tic-Tac-Toe - Pure game logic with formal verification
//!
//! This crate provides the core tic-tac-toe game types and rules with zero
//! application dependencies. Designed for formal verification with Kani, Verus,
//! and Creusot.
//!
//! ## Architecture
//!
//! - **Pure types**: Board, Player, Position, Square
//! - **Pure rules**: Win/draw detection, move validation
//! - **Typestate**: GameSetup, GameInProgress, GameFinished with proof-carrying contracts
//! - **Elicitation derives**: Generates verification harness methods
//! - **Zero deps**: Only elicitation + serde

#![warn(missing_docs)]

pub mod action;
pub mod contracts;
mod complete;
mod explore;
pub mod outcome;
pub mod position;
pub mod rules;
pub mod typestate;
pub mod types;
mod view;

// Re-export core types
pub use action::{Move, MoveError};
pub use contracts::{
    LegalMove, PlayerTurn, SquareEmpty, execute_move, validate_move, validate_player_turn,
    validate_square_empty,
};
pub use explore::TicTacToeAction;
pub use outcome::Outcome;
pub use position::Position;
pub use typestate::{GameFinished, GameInProgress, GameResult, GameSetup};
pub use types::{Board, Player, Square};
pub use view::TicTacToeView;
