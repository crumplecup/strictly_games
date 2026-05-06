//! Gallery level SGC4: full multi-state lifecycle with typestate transitions.
//!
//! **Hypothesis**: The VSM companion pattern — `#[requires(inv(&state))]`
//! `#[ensures(inv(&result))]` on transitions that unwrap a specific variant
//! and return another — works end-to-end for a 3-state mini game enum that
//! wraps the `MiniInProgress` struct from SGC2/3.
//!
//! This directly mirrors the shape of the real `TicTacToeState` companion in
//! `strictly_proofs/src/creusot_proofs/generated/tictactoe_vsm.rs`.
//!
//! ## State machine
//!
//! ```text
//! Setup ──begin──► InProgress ──push──► InProgress
//!                       └──finish(≥1 move)──► Done
//! ```
//!
//! ## Invariant
//!
//! ```text
//! sgc4_invariant(s) ≡
//!     Setup       => true
//!     InProgress  => history.len() <= 9
//!     Done        => history.len() >= 1
//! ```
//!
//! ## Experiment table
//!
//! | ID     | What                                               | Expected |
//! |--------|-----------------------------------------------------|----------|
//! | SGC4a  | Constructor: `Setup` satisfies invariant            | ✓        |
//! | SGC4b  | `begin`: `Setup` → `InProgress`, inv holds          | ✓        |
//! | SGC4c  | `push_move`: `InProgress` (room) → `InProgress`    | ✓        |
//! | SGC4d  | `finish`: `InProgress` (≥1 move) → `Done`          | ✓        |
//! | SGC4e  | Helper predicates: `sgc4_has_room`, `sgc4_has_move` | ✓        |

use creusot_std::prelude::*;

use crate::creusot_proofs::gallery::level2::MiniInProgress;

// ── Mini game enum ─────────────────────────────────────────────────────────────

/// Inline mini game typestate mirroring the `TicTacToeState` shape.
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

// ── Invariant and helper predicates ───────────────────────────────────────────

/// SGC4 invariant: mirrors `tictactoe_consistent`.
///
/// - `Setup`:      always valid.
/// - `InProgress`: history bounded to 9 (board size).
/// - `Done`:       at least 1 move was played.
#[logic]
pub fn sgc4_invariant(state: &MiniState) -> bool {
    pearlite! {
        match state {
            MiniState::Setup                => true,
            MiniState::InProgress { inner } => inner.history@.len() <= 9,
            MiniState::Done       { inner } => inner.history@.len() >= 1,
        }
    }
}

/// SGC4e: true iff the state is `InProgress` with room for another move.
#[logic]
pub fn sgc4_has_room(state: &MiniState) -> bool {
    pearlite! {
        match state {
            MiniState::InProgress { inner } => inner.history@.len() < 9,
            _                               => false,
        }
    }
}

/// SGC4e: true iff the state is `InProgress` with at least one move made.
#[logic]
pub fn sgc4_has_move(state: &MiniState) -> bool {
    pearlite! {
        match state {
            MiniState::InProgress { inner } => inner.history@.len() >= 1,
            _                               => false,
        }
    }
}

// ── Transitions ───────────────────────────────────────────────────────────────

/// SGC4a: constructor — produces a valid `Setup` state.
#[requires(true)]
#[ensures(sgc4_invariant(&result))]
pub fn sgc4_new() -> MiniState {
    MiniState::Setup
}

/// SGC4b: begin — `Setup` → `InProgress { history: [] }`.
///
/// All other variants pass through unchanged.
#[requires(sgc4_invariant(&state))]
#[ensures(sgc4_invariant(&result))]
pub fn sgc4_begin(state: MiniState) -> MiniState {
    match state {
        MiniState::Setup => MiniState::InProgress {
            inner: MiniInProgress {
                history: Vec::new(),
            },
        },
        other => other,
    }
}

/// SGC4c: push a move — `InProgress` (with room) → `InProgress`.
///
/// Requires `sgc4_has_room` as an additional guard so Creusot can discharge
/// the `history@.len() + 1 <= 9` postcondition.
#[requires(sgc4_invariant(&state) && sgc4_has_room(&state))]
#[ensures(sgc4_invariant(&result))]
pub fn sgc4_push(state: MiniState, mv: u8) -> MiniState {
    let MiniState::InProgress { mut inner } = state else {
        return state;
    };
    inner.history.push(mv);
    MiniState::InProgress { inner }
}

/// SGC4d: finish — `InProgress` (with at least 1 move) → `Done`.
///
/// The `sgc4_has_move` guard ensures the `Done` variant's `>= 1` invariant.
#[requires(sgc4_invariant(&state) && sgc4_has_move(&state))]
#[ensures(sgc4_invariant(&result))]
pub fn sgc4_finish(state: MiniState) -> MiniState {
    let MiniState::InProgress { inner } = state else {
        return state;
    };
    MiniState::Done { inner }
}
