//! Blackjack game — server orchestration layer.
//!
//! Game logic, typestate, and workflow contracts live in `strictly_blackjack`.
//! This module provides the async `BlackjackWorkflow<C>` orchestrator that
//! bridges the game logic with `ElicitCommunicator` for human and AI sessions.

pub mod workflow;

pub use workflow::{BlackjackWorkflow, HandResult};
