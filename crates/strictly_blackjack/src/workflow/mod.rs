//! Blackjack workflow — proof-carrying phase transitions.
//!
//! Exports the propositions and tools that express the blackjack turn sequence
//! as elicitation contract compositions with explicit Pre/Post propositions.

mod propositions;
mod tools;

pub use propositions::{BetPlaced, PlayerTurnComplete};
pub use tools::{
    PlaceBetOutput, PlayActionOutput, PlayActionResult, execute_dealer_turn, execute_place_bet,
    execute_play_action,
};
