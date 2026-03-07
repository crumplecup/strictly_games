//! Strictly Blackjack - Pure game logic with formal verification
//!
//! This crate provides the core blackjack game types and rules with zero
//! application dependencies. Designed for formal verification with Kani, Verus,
//! and Creusot.
//!
//! ## Architecture
//!
//! - **Pure types**: Card, Rank, Suit, Deck, Hand
//! - **Pure rules**: Hand value calculation, blackjack detection, bust detection
//! - **Elicitation derives**: Generates verification harness methods
//! - **Zero deps**: Only elicitation + serde

#![warn(missing_docs)]

mod card;
mod deck;
mod hand;
pub mod rules;
mod types;

// Re-export core types
pub use card::{Card, Rank, Suit};
pub use deck::Deck;
pub use hand::{Hand, HandValue};
pub use types::Outcome;
