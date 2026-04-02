//! Player action types for blackjack.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Basic actions available to the player.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
pub enum BasicAction {
    /// Take another card.
    Hit,
    /// Keep current hand and end turn.
    Stand,
}

impl BasicAction {
    /// Returns the display label for this action.
    pub fn label(self) -> &'static str {
        match self {
            BasicAction::Hit => "Hit",
            BasicAction::Stand => "Stand",
        }
    }
}

impl std::fmt::Display for BasicAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A player action with context for validation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Elicit, schemars::JsonSchema,
)]
pub struct PlayerAction {
    action: BasicAction,
    hand_index: usize,
}

impl PlayerAction {
    /// Creates a new player action.
    #[instrument]
    pub fn new(action: BasicAction, hand_index: usize) -> Self {
        Self { action, hand_index }
    }

    /// Returns the action.
    pub fn action(&self) -> BasicAction {
        self.action
    }

    /// Returns the hand index.
    pub fn hand_index(&self) -> usize {
        self.hand_index
    }
}
