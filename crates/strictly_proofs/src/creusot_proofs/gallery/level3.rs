//! Gallery level SGC3: `Vec::push` transition preserving `@.len() <= 9`.
//!
//! **Hypothesis**: Creusot can reason about `Vec::push` incrementing `@.len()`
//! by exactly 1, and can discharge the postcondition
//! `result.history@.len() == inner.history@.len() + 1` given a
//! `#[requires(inner.history@.len() < 9)]` guard.
//!
//! This validates the key reasoning step for any transition that appends a
//! move (tictactoe `make_move`, craps `point_roll`, etc.).
//!
//! ## Experiment table
//!
//! | ID     | What                                                     | Expected |
//! |--------|----------------------------------------------------------|----------|
//! | SGC3a  | `push` under `< 9` guard: `len + 1 <= 9` holds          | ✓        |
//! | SGC3b  | Exact postcondition: `result.history@.len() == old + 1` | ✓        |

use creusot_std::prelude::*;

use crate::creusot_proofs::gallery::level2::{MiniInProgress, sgc2_invariant};

/// SGC3a/b: append a move when there is still room on the board.
///
/// The `history@.len() < 9` guard ensures push doesn't violate the bound.
/// Creusot's model of `Vec::push` knows it increments `@.len()` by exactly 1.
#[requires(sgc2_invariant(&inner) && inner.history@.len() < 9)]
#[ensures(sgc2_invariant(&result))]
#[ensures(result.history@.len() == inner.history@.len() + 1)]
pub fn sgc3_push(mut inner: MiniInProgress, mv: u8) -> MiniInProgress {
    inner.history.push(mv);
    inner
}
