//! Blackjack workflow — async orchestration via `BlackjackWorkflow<C>`.
//!
//! Game logic and proof-carrying contracts live in `strictly_blackjack`.
//! This module wraps them with an async communicator loop for human and AI sessions.

mod runner;

pub use runner::{BlackjackWorkflow, HandResult};
