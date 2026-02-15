//! Phase marker types for typestate state machine.
//!
//! These types exist solely as type-level markers to encode
//! game phase in the type system. They have no runtime representation.

use serde::{Deserialize, Serialize};

/// Phase marker: Game is being set up (players not assigned).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Setup;

/// Phase marker: Game is in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InProgress;

/// Phase marker: Game has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Finished;

/// Outcome of a finished game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    /// Player won the game.
    Winner(super::Player),
    /// Game ended in a draw.
    Draw,
}

impl Outcome {
    /// Returns the winner if there is one.
    pub fn winner(&self) -> Option<super::Player> {
        match self {
            Outcome::Winner(player) => Some(*player),
            Outcome::Draw => None,
        }
    }
    
    /// Returns true if the game was a draw.
    pub fn is_draw(&self) -> bool {
        matches!(self, Outcome::Draw)
    }
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Winner(player) => write!(f, "Player {:?} wins", player),
            Outcome::Draw => write!(f, "Draw"),
        }
    }
}
