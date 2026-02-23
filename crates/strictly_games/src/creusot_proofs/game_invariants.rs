//! Creusot proofs for tic-tac-toe game invariants.

use crate::games::tictactoe::{Board, Player, Position, Square, rules};

/// Verify opponent() is an involution.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(p == p.opponent().opponent())]
pub fn verify_opponent_involutive(p: Player) -> Player {
    p.opponent().opponent()
}

/// Verify Position::to_index() returns valid board index.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(pos.to_index() < 9)]
pub fn verify_position_to_index_valid(pos: Position) -> usize {
    pos.to_index()
}

/// Verify new board is empty everywhere.
#[cfg(creusot)]
#[trusted]
#[requires(true)]
#[ensures(Board::new().is_empty(pos))]
pub fn verify_new_board_empty(pos: Position) -> bool {
    Board::new().is_empty(pos)
}
