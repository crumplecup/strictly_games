//! Blackjack workflow — proof-carrying phase transitions.
//!
//! Expresses blackjack as elicitation contract compositions with explicit
//! `Pre`/`Post` propositions.  The same `BlackjackWorkflow<C>` drives both
//! human TUI sessions and AI agent sessions; only the communicator differs.
//!
//! # Phase contract chain
//!
//! ```text
//! True → execute_place_bet → BetPlaced → execute_play_action (loop) → PlayerTurnComplete
//!                                                                             ↓
//!                                                         execute_dealer_turn → PayoutSettled
//! ```

mod propositions;
mod runner;
mod tools;

pub use propositions::{BetPlaced, PayoutSettled, PlayerTurnComplete};
pub use runner::{BlackjackWorkflow, HandResult};
pub use tools::{
    PlaceBetOutput, PlayActionOutput, PlayActionResult, execute_dealer_turn, execute_place_bet,
    execute_play_action,
};
