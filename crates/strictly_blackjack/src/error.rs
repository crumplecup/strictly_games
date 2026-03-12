//! Error types for blackjack game actions.

use derive_more::{Display, Error};

/// Errors that can occur during blackjack game actions.
#[derive(Debug, Clone, PartialEq, Eq, Display, Error)]
pub enum ActionError {
    /// Hand is already bust.
    #[display("Hand is already bust")]
    HandBust,

    /// Invalid hand index.
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
