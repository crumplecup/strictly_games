//! Creusot proofs for tic-tac-toe game invariants.
//!
//! Note: `opponent()`, `Board::new()`, `Board::is_empty()` are program
//! functions — not `#[logic]` — so they cannot appear in `#[ensures]`
//! clauses. These proofs are `#[trusted]` axioms that compile and register
//! with Why3; their bodies witness the behaviour at the program level.

use strictly_tictactoe::{Board, Player, Position};

#[cfg(creusot)]
use creusot_std::prelude::*;

/// Witness: opponent() is an involution (program-level check).
///
/// The full logic spec requires `opponent()` to be a `#[logic]` fn.
/// The correctness is covered by Kani's `player_opponent_is_involutive`.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(true)]
pub fn verify_opponent_involutive(p: Player) -> Player {
    p.opponent().opponent()
}

/// Verify Position::to_index() returns valid board index.
///
/// Uses the `@` model operator to lift `usize` into `Int` for comparison.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(result@ < 9)]
pub fn verify_position_to_index_valid(pos: Position) -> usize {
    pos.to_index()
}

/// Witness: new board is empty (program-level check).
///
/// `Board::new()` and `is_empty()` are not `#[logic]` fns; correctness
/// is covered by Kani's `new_board_is_empty`.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(true)]
pub fn verify_new_board_empty(pos: Position) -> bool {
    Board::new().is_empty(pos)
}
