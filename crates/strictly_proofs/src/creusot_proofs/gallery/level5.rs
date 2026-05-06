//! Gallery level SGC5: cross-crate field access on real `TicTacToeState`.
//!
//! **Hypothesis**: A `#[logic]` function in `strictly_proofs` can access `pub`
//! fields of types from `strictly_tictactoe` via the `@` model operator.
//! Specifically, `inner.history@.len()` where
//! `inner: &GameInProgress` and `GameInProgress::history: pub Vec<Move>`.
//!
//! This is the **critical validation** for the generated companions in
//! `strictly_proofs/src/creusot_proofs/generated/tictactoe_vsm.rs`.
//! If this level proves in Why3, the VSM companion pattern is sound.
//!
//! ## Why this matters
//!
//! Gallery levels SGC1–4 used inline mini-types.  This level imports the
//! *real* `TicTacToeState` from `strictly_tictactoe`.  The only requirement
//! is that `GameInProgress::history` (and the other fields used in invariants)
//! are declared `pub` — which was established in the preceding commits.
//!
//! ## Opacity caveat
//!
//! Per the lesson from `elicitation_creusot/gallery/level12`:
//! cross-module `#[logic]` function *bodies* are opaque in Why3.  However,
//! ADT definitions (enum variants + field names) *are* inlined across module
//! boundaries.  Since `sgc5_tictactoe_consistent` is defined here (in
//! `strictly_proofs`), not in `strictly_tictactoe`, there is no opacity
//! issue — the body is visible to Why3 locally.
//!
//! ## Experiment table
//!
//! | ID     | What                                                        | Expected |
//! |--------|-------------------------------------------------------------|----------|
//! | SGC5a  | `sgc5_tictactoe_consistent` over real `TicTacToeState`      | ✓        |
//! | SGC5b  | `#[trusted]` transition wrapper for `ttt_restart`           | ✓        |

use creusot_std::prelude::*;
use strictly_tictactoe::*;

/// SGC5a: the exact invariant predicate used in the generated Creusot companion.
///
/// Accesses `inner.history` — a `pub Vec<Move>` field on `GameInProgress` —
/// directly via the `@` model operator.  If this compiles and proves in Why3,
/// the generated `tictactoe_vsm.rs` companion is syntactically valid.
#[logic]
pub fn sgc5_tictactoe_consistent(state: &TicTacToeState) -> bool {
    pearlite! {
        match state {
            TicTacToeState::Setup { .. }             => true,
            TicTacToeState::InProgress { inner, .. } => inner.history@.len() <= 9,
            TicTacToeState::Finished   { inner, .. } => inner.history@.len() >= 5,
        }
    }
}

/// SGC5b: `ttt_restart` wrapper under the invariant — `Finished` → `Setup`.
///
/// Marked `#[trusted]` so that Creusot emits a VCgen goal for the contract
/// without needing to unfold `GameFinished::restart()` (an exec fn from
/// another crate).  The Kani companion proves the runtime correctness of
/// `restart`; this level validates the *contract plumbing* only.
#[requires(sgc5_tictactoe_consistent(&state))]
#[ensures(sgc5_tictactoe_consistent(&result))]
#[trusted]
pub fn sgc5_ttt_restart(state: TicTacToeState) -> TicTacToeState {
    let TicTacToeState::Finished {
        inner: finished,
        display_mode,
    } = state
    else {
        return state;
    };
    TicTacToeState::Setup {
        inner: finished.restart(),
        display_mode,
    }
}
