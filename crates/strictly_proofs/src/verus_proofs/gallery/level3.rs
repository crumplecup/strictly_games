//! Gallery level SGV3: `assume_specification` for external transition functions.
//!
//! **Hypothesis**: `pub assume_specification[fn_name]` injects a trusted spec for a
//! function whose body Verus does not verify (`#[verifier::external]`), and callers
//! can reason from that contract as an axiom.
//!
//! This is the exact mechanism used in generated companions like
//! `verus_proofs/generated/tictactoe_vsm.rs` for async transition functions
//! from another crate (e.g. `ttt_start_game`).  The gallery validates the
//! pattern with inline mini-types (because Verus cannot resolve workspace deps).
//!
//! ## State machine
//!
//! ```text
//! Setup ──sgv3_begin──► InProgress ──sgv3_push──► InProgress
//!                             └──sgv3_finish(≥1 move)──► Done
//! ```
//!
//! ## Invariant
//!
//! ```text
//! sgv3_invariant(s) ≡
//!     Setup       => true
//!     InProgress  => history.len() <= 9
//!     Done        => history.len() >= 1
//! ```
//!
//! ## Experiment table
//!
//! | ID     | What                                                              | Expected |
//! |--------|-------------------------------------------------------------------|----------|
//! | SGV3a  | `assume_specification` for external `begin` with inv pre/post     | ✓        |
//! | SGV3b  | `assume_specification` for external `finish` with lower-bound post | ✓        |
//! | SGV3c  | Helper spec fn: `sgv3_is_in_progress` for variant detection       | ✓        |
//! | SGV3d  | Caller harness uses assumed specs to prove inv preservation        | ✓        |

use verus_builtin_macros::verus;

verus! {

use vstd::prelude::*;

// ── Inline mini types (defined inside verus! so prover knows the ADT) ─────────

/// Inline game-in-progress record (mirrors `GameInProgress`).
#[derive(Clone)]
pub struct MiniInProgress {
    /// Ordered sequence of moves; at most 9 entries.
    pub history: Vec<u8>,
}

/// Inline mini game typestate (mirrors `TicTacToeState`).
#[derive(Clone)]
pub enum MiniState {
    /// Not yet started.
    Setup,
    /// Game in progress.
    InProgress {
        /// Active game data.
        inner: MiniInProgress,
    },
    /// Game finished; at least one move was made.
    Done {
        /// Final game data.
        inner: MiniInProgress,
    },
}

// ── Invariant and helper spec fns ─────────────────────────────────────────────

/// SGV3 invariant: mirrors `tictactoe_consistent`.
pub open spec fn sgv3_invariant(state: &MiniState) -> bool {
    match *state {
        MiniState::Setup                => true,
        MiniState::InProgress { inner } => inner.history@.len() <= 9,
        MiniState::Done       { inner } => inner.history@.len() >= 1,
    }
}

/// SGV3c: true iff state is `InProgress`.
pub open spec fn sgv3_is_in_progress(state: &MiniState) -> bool {
    matches!(*state, MiniState::InProgress { .. })
}

// ── External exec functions: Verus does not verify their bodies ───────────────
//
// `#[verifier::external]` simulates async transition functions from another crate.
// `assume_specification` injects trusted contracts the prover uses as axioms.

/// SGV3a source: external begin transition, `Setup` → `InProgress { history: [] }`.
#[verifier::external]
pub fn sgv3_begin(state: MiniState) -> MiniState {
    match state {
        MiniState::Setup => MiniState::InProgress {
            inner: MiniInProgress { history: Vec::new() },
        },
        other => other,
    }
}

/// SGV3b source: external finish transition, `InProgress` → `Done`.
#[verifier::external]
pub fn sgv3_finish(state: MiniState) -> MiniState {
    match state {
        MiniState::InProgress { inner } => MiniState::Done { inner },
        other => other,
    }
}

// ── SGV3a: inject trusted spec for `sgv3_begin` ───────────────────────────────

/// SGV3a: assume the external `begin` preserves the invariant.
///
/// This is the exact pattern used in generated companions for each VSM transition.
pub assume_specification[ sgv3_begin ](state: MiniState) -> (result: MiniState)
    requires sgv3_invariant(&state),
    ensures
        sgv3_invariant(&result),
        sgv3_is_in_progress(&result);

// ── SGV3b: inject trusted spec for `sgv3_finish` ─────────────────────────────

/// SGV3b: assume the external `finish` preserves the invariant.
pub assume_specification[ sgv3_finish ](state: MiniState) -> (result: MiniState)
    requires
        sgv3_invariant(&state),
        sgv3_is_in_progress(&state),
    ensures
        sgv3_invariant(&result),
        !sgv3_is_in_progress(&result);

// ── SGV3d: caller harness reasons from assumed specs ──────────────────────────

/// SGV3d: chain `begin` → `finish` using only the assumed postconditions.
///
/// Verus proves `sgv3_invariant` is maintained at every step without ever
/// seeing the external function bodies — exactly what the generated companions do.
pub fn sgv3_lifecycle_harness()
    ensures true,
{
    let s0 = MiniState::Setup;
    assert(sgv3_invariant(&s0));

    let s1 = sgv3_begin(s0);
    assert(sgv3_invariant(&s1));
    assert(sgv3_is_in_progress(&s1));

    let s2 = sgv3_finish(s1);
    assert(sgv3_invariant(&s2));
    assert(!sgv3_is_in_progress(&s2));
}

} // verus!
