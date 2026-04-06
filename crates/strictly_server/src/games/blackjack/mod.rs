//! Blackjack game — server orchestration layer.
//!
//! Game logic, typestate, and workflow contracts live in `strictly_blackjack`.
//! This module provides the async `BlackjackWorkflow<C>` orchestrator that
//! bridges the game logic with `ElicitCommunicator` for human and AI sessions.

pub mod factories;
pub mod session;
pub mod workflow;

pub use factories::{BetConstraints, DEFAULT_PRESETS, register_bet_tools};
pub use session::{BlackjackPhase, BlackjackSession, BlackjackStateView, new_session};
pub use workflow::{BlackjackWorkflow, HandResult};
