//! Gallery level SGC1: inline mini-game enum, scalar `@` invariant.
//!
//! **Hypothesis**: `pearlite!{ match state { Variant { count } => count@ <= 9, _ => true } }`
//! works as a `#[logic]` predicate over an enum with integer fields.
//! This is the foundational `@` pattern used in every game VSM invariant.
//!
//! ## Experiment table
//!
//! | ID     | What                                        | Expected |
//! |--------|---------------------------------------------|----------|
//! | SGC1a  | Trivial arm (`Setup => true`)               | ✓        |
//! | SGC1b  | Upper-bound arm (`Active => count@ <= 9`)   | ✓        |
//! | SGC1c  | Lower-bound arm (`Done => count@ >= 1`)     | ✓        |
//! | SGC1d  | Constructor establishes invariant           | ✓        |
//! | SGC1e  | Identity transition preserves invariant     | ✓        |
//! | SGC1f  | `begin` — `Setup` → `Active { count: 0 }`  | ✓        |

use creusot_std::prelude::*;

/// Mini game state — single active phase with a move counter.
pub enum MiniGameState {
    /// No game in progress; no constraints.
    Setup,
    /// Game is being played; `count` moves have been made so far.
    Active {
        /// Number of moves made; must not exceed 9 (a 3×3 board).
        count: usize,
    },
    /// Game has ended; at least one move was made.
    Done {
        /// Final move count; must be ≥ 1.
        count: usize,
    },
}

/// SGC1a/b/c: invariant over all three variants.
///
/// - `Setup`:  always valid — no data yet.
/// - `Active`: at most 9 moves (board is 3×3).
/// - `Done`:   at least 1 move was played.
#[logic]
pub fn sgc1_invariant(state: &MiniGameState) -> bool {
    pearlite! {
        match state {
            MiniGameState::Setup             => true,
            MiniGameState::Active { count }  => count@ <= 9,
            MiniGameState::Done   { count }  => count@ >= 1,
        }
    }
}

/// SGC1d: constructor — always produces a valid initial state.
#[requires(true)]
#[ensures(sgc1_invariant(&result))]
pub fn sgc1_new() -> MiniGameState {
    MiniGameState::Setup
}

/// SGC1e: identity — passes any valid state through unchanged.
#[requires(sgc1_invariant(&state))]
#[ensures(sgc1_invariant(&result))]
pub fn sgc1_identity(state: MiniGameState) -> MiniGameState {
    state
}

/// SGC1f: begin — `Setup` → `Active { count: 0 }`.
///
/// Postcondition: `count@ == 0 ≤ 9`, so `sgc1_invariant` holds.
#[requires(sgc1_invariant(&state))]
#[ensures(sgc1_invariant(&result))]
pub fn sgc1_begin(state: MiniGameState) -> MiniGameState {
    match state {
        MiniGameState::Setup => MiniGameState::Active { count: 0 },
        other => other,
    }
}
