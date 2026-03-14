//! Error types for blackjack game actions.

use derive_more::{Display, Error};

/// Errors that can occur during blackjack game actions.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
pub enum ActionError {
    /// Hand is already bust.
    #[display("Hand is already bust")]
    HandBust,

    /// Action targeted the wrong hand — it is not this hand's turn.
    ///
    /// Use [`crate::GamePlayerTurn::action_on_current`] to avoid this error:
    /// it automatically targets the current hand.
    #[display("Wrong hand turn: expected hand {expected}, got {got}")]
    WrongHandTurn {
        /// The hand index that should have been targeted.
        expected: usize,
        /// The hand index that was actually provided.
        got: usize,
    },

    /// Hand index is out of bounds.
    #[display("Invalid hand index: {}", _0)]
    InvalidHandIndex(#[error(not(source))] usize),

    /// Insufficient funds for bet.
    #[display("Insufficient funds: need {}, have {}", _0, _1)]
    InsufficientFunds(#[error(not(source))] u64, #[error(not(source))] u64),

    /// Invalid bet amount.
    #[display("Invalid bet amount: {}", _0)]
    InvalidBet(#[error(not(source))] u64),

    /// Deck exhausted.
    #[display("No cards remaining in deck")]
    DeckExhausted,
}
