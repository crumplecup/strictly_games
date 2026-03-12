//! Action types for blackjack.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Basic actions available to the player (Milestone 1: Hit/Stand only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Elicit, schemars::JsonSchema)]
pub enum BasicAction {
    /// Take another card.
    Hit,
    /// Keep current hand and end turn.
    Stand,
}

impl BasicAction {
    /// Returns the label for this action.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Elicit)]
pub struct PlayerAction {
    action: BasicAction,
    hand_index: usize,
}

impl PlayerAction {
    /// Creates a new player action.
    #[instrument]
    pub fn new(action: BasicAction, hand_index: usize) -> Self {
        Self {
            action,
            hand_index,
        }
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

/// Errors that can occur when taking an action.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum ActionError {
    /// Hand is already bust.
    #[display("Hand is already bust")]
    HandBust,

    /// Invalid hand index.
    #[display("Invalid hand index: {}", _0)]
    InvalidHandIndex(usize),

    /// Insufficient funds for bet.
    #[display("Insufficient funds: need {}, have {}", _0, _1)]
    InsufficientFunds(u64, u64),

    /// Invalid bet amount.
    #[display("Invalid bet amount: {}", _0)]
    InvalidBet(u64),

    /// Deck exhausted.
    #[display("No cards remaining in deck")]
    DeckExhausted,
}

impl std::error::Error for ActionError {}
