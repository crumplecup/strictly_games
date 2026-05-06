//! Gallery level SGV2: struct with `Vec<T>@.len()` bound in Verus spec fn.
//!
//! **Hypothesis**: `inner.history@.len() <= 9` in a `pub open spec fn`
//! works on a `pub Vec<u8>` field via `vstd`'s `View` impl for `Vec<T>`
//! (which maps to `Seq<T>`).  `.len()` on a Verus `Seq` returns a `nat`.
//!
//! This is the exact expression used in `tictactoe_consistent` for the
//! `InProgress` variant in the Verus generated companion.
//!
//! ## Experiment table
//!
//! | ID     | What                                              | Expected |
//! |--------|---------------------------------------------------|----------|
//! | SGV2a  | `history@.len() <= 9` in spec fn on struct field | ✓        |
//! | SGV2b  | Constructor: empty Vec has `@.len() == 0`         | ✓        |
//! | SGV2c  | Push under guard: `len + 1 <= 9`                  | ✓        |

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

/// A game in progress that records each move.
#[derive(Debug, Clone)]
pub struct MiniInProgress {
    /// Ordered sequence of moves; at most 9 entries.
    pub history: Vec<u8>,
}

/// SGV2a: history length bounded by board size.
pub open spec fn sgv2_invariant(inner: &MiniInProgress) -> bool {
    inner.history@.len() <= 9
}

/// SGV2b: constructor — empty history trivially satisfies the bound.
pub fn sgv2_new() -> (result: MiniInProgress)
    ensures
        sgv2_invariant(&result),
        result.history@.len() == 0,
{
    MiniInProgress {
        history: Vec::new(),
    }
}

/// SGV2c: push a move when there is room.
///
/// The `history@.len() < 9` guard allows Verus to discharge
/// `result.history@.len() == old_len + 1 <= 9`.
pub fn sgv2_push(mut inner: MiniInProgress, mv: u8) -> (result: MiniInProgress)
    requires
        sgv2_invariant(&inner),
        inner.history@.len() < 9,
    ensures
        sgv2_invariant(&result),
        result.history@.len() == inner.history@.len() + 1,
{
    inner.history.push(mv);
    inner
}

} // verus!
