//! Gallery level SGC2: struct with `Vec<T>@.len()` invariant.
//!
//! **Hypothesis**: `pearlite!{ inner.history@.len() <= 9 }` compiles and
//! type-checks on a `pub Vec<u8>` field.  This is the exact expression used
//! in `tictactoe_consistent` for the `InProgress` variant.
//!
//! The `@` operator on `Vec<T>` yields a `Seq<T>` in the Creusot model.
//! `.len()` on a `Seq` returns a Pearlite `Int`.  Comparing against the
//! integer literal `9` works because Pearlite coerces integer literals to
//! `Int`.
//!
//! ## Experiment table
//!
//! | ID     | What                                            | Expected |
//! |--------|--------------------------------------------------|----------|
//! | SGC2a  | Empty `Vec`: `@.len() == 0 <= 9`               | ✓        |
//! | SGC2b  | `#[logic]` predicate on struct with `pub Vec`  | ✓        |
//! | SGC2c  | Constructor: empty history satisfies bound     | ✓        |
//! | SGC2d  | Identity: preserves invariant                  | ✓        |

use creusot_std::prelude::*;

/// A game in progress that records each move as a byte.
pub struct MiniInProgress {
    /// Ordered sequence of moves; at most 9 entries (a 3×3 board).
    pub history: Vec<u8>,
}

/// SGC2a/b: history length is bounded by the board size.
#[logic]
pub fn sgc2_invariant(inner: &MiniInProgress) -> bool {
    pearlite! {
        inner.history@.len() <= 9
    }
}

/// SGC2c: fresh game — empty history trivially satisfies the bound.
#[requires(true)]
#[ensures(sgc2_invariant(&result))]
#[ensures(result.history@.len() == 0)]
pub fn sgc2_new() -> MiniInProgress {
    MiniInProgress {
        history: Vec::new(),
    }
}

/// SGC2d: identity — passes a valid game through unchanged.
#[requires(sgc2_invariant(&inner))]
#[ensures(sgc2_invariant(&result))]
pub fn sgc2_identity(inner: MiniInProgress) -> MiniInProgress {
    inner
}
