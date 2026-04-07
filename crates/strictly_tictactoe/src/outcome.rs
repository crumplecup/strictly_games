//! Game outcome type.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};

/// Outcome of a finished game.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
pub enum Outcome {
    /// Player won the game.
    Winner(crate::Player),
    /// Game ended in a draw.
    Draw,
}

impl Outcome {
    /// Returns the winner if there is one.
    pub fn winner(&self) -> Option<crate::Player> {
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
