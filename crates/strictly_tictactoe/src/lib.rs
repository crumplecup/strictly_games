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
//! - **Elicitation derives**: Generates verification harness methods
//! - **Zero deps**: Only elicitation + serde

#![warn(missing_docs)]

mod complete;
pub mod position;
pub mod rules;
pub mod types;

// Re-export core types
pub use position::Position;
pub use types::{Board, Player, Square};
