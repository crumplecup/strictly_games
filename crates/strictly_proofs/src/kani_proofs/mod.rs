//! Kani formal verification proofs.
//!
//! This module contains proof harnesses demonstrating that the game
//! implementation is formally verified through compositional reasoning.

/// Auto-generated foundation proofs from `Type::kani_proof()` composition.
///
/// These are the layer-1 structural proofs emitted by `build.rs`.
/// Domain-specific harnesses in the sibling modules build on top of them.
#[cfg(kani)]
pub mod generated;

pub mod bankroll_financial;
pub mod blackjack_compositional;
pub mod blackjack_invariants;
pub mod blackjack_scenarios;
pub mod compositional_proof;
pub mod craps_financial;
pub mod craps_invariants;
pub mod craps_scenarios;
pub mod game_invariants;
pub mod tictactoe_contracts;
pub mod tui_breakpoints;
