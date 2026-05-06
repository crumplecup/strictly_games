//! Gallery level SGV1: inline mini-game enum, `match *state` invariant.
//!
//! **Hypothesis**: `pub open spec fn inv(state: &MiniState) -> bool` with a
//! `match *state { Active { count, .. } => count@ <= 9, ... }` body works
//! in Verus.  The `*state` deref is required because the function takes a
//! reference, but `match` in spec context needs a value.
//!
//! This is the foundational deref-match pattern used in every Verus game
//! VSM companion (`verus_proofs/generated/tictactoe_vsm.rs` etc.).
//!
//! ## Experiment table
//!
//! | ID     | What                                           | Expected |
//! |--------|------------------------------------------------|----------|
//! | SGV1a  | `match *state` deref on enum reference         | ✓        |
//! | SGV1b  | Scalar `count@` bound in spec fn               | ✓        |
//! | SGV1c  | Constructor establishes invariant              | ✓        |
//! | SGV1d  | Identity transition preserves invariant        | ✓        |
//! | SGV1e  | `begin` — `Setup` → `Active { count: 0 }`     | ✓        |

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

/// Mini game state — single active phase with a move counter.
#[derive(Debug, Clone)]
pub enum MiniGameState {
    /// No game in progress.
    Setup,
    /// Game is being played; `count` moves made so far.
    Active {
        /// Moves made; must not exceed 9 (3×3 board).
        count: usize,
    },
    /// Game over; at least one move was played.
    Done {
        /// Final move count; at least 1.
        count: usize,
    },
}

/// SGV1a/b: invariant using the `match *state` deref pattern.
///
/// - `Setup`:  trivially valid.
/// - `Active`: at most 9 moves.
/// - `Done`:   at least 1 move.
pub open spec fn sgv1_invariant(state: &MiniGameState) -> bool {
    match *state {
        MiniGameState::Setup             => true,
        MiniGameState::Active { count }  => count@ <= 9,
        MiniGameState::Done   { count }  => count@ >= 1,
    }
}

/// SGV1c: constructor — always produces a valid initial state.
pub fn sgv1_new() -> (result: MiniGameState)
    ensures sgv1_invariant(&result),
{
    MiniGameState::Setup
}

/// SGV1d: identity — passes a valid state through unchanged.
pub fn sgv1_identity(state: MiniGameState) -> (result: MiniGameState)
    requires sgv1_invariant(&state),
    ensures  sgv1_invariant(&result),
{
    state
}

/// SGV1e: begin — `Setup` → `Active { count: 0 }`.
///
/// Postcondition: `count@ == 0 ≤ 9`.
pub fn sgv1_begin(state: MiniGameState) -> (result: MiniGameState)
    requires sgv1_invariant(&state),
    ensures  sgv1_invariant(&result),
{
    match state {
        MiniGameState::Setup => MiniGameState::Active { count: 0 },
        other => other,
    }
}

} // verus!
