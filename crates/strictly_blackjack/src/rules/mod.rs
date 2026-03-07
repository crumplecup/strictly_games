//! Game rules for blackjack.
//!
//! This module contains pure functions for evaluating game state
//! according to blackjack rules. Rules are separated from hand
//! storage to enable composition into contract systems.

pub mod blackjack;
pub mod bust;
pub mod hand_value;

pub use blackjack::is_blackjack;
pub use bust::is_bust;
pub use hand_value::calculate_value;
