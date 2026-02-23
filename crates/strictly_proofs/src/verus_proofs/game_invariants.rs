//! Verus proofs for tic-tac-toe game invariants.

#[cfg(verus)]
use verus_builtin::*;
#[cfg(verus)]
use verus_builtin_macros::*;

use strictly_tictactoe::{Board, Player, Position, Square, rules};

#[cfg(verus)]
verus! {

/// Verify opponent() is an involution: opponent(opponent(p)) = p
pub proof fn verify_opponent_involutive(p: Player)
    ensures p == p.opponent().opponent(),
{
    match p {
        Player::X => {
            assert(p.opponent() == Player::O);
            assert(p.opponent().opponent() == Player::X);
        }
        Player::O => {
            assert(p.opponent() == Player::X);
            assert(p.opponent().opponent() == Player::O);
        }
    }
}

/// Verify Position::to_index() returns valid board index
pub proof fn verify_position_to_index_valid(pos: Position)
    ensures pos.to_index() < 9,
{
    match pos {
        Position::TopLeft => assert(pos.to_index() == 0),
        Position::TopCenter => assert(pos.to_index() == 1),
        Position::TopRight => assert(pos.to_index() == 2),
        Position::MiddleLeft => assert(pos.to_index() == 3),
        Position::Center => assert(pos.to_index() == 4),
        Position::MiddleRight => assert(pos.to_index() == 5),
        Position::BottomLeft => assert(pos.to_index() == 6),
        Position::BottomCenter => assert(pos.to_index() == 7),
        Position::BottomRight => assert(pos.to_index() == 8),
    }
}

/// Verify new board is empty everywhere
pub proof fn verify_new_board_empty(pos: Position)
    ensures Board::new().is_empty(pos),
{
    let board = Board::new();
    assert(board.get(pos) == Square::Empty);
    assert(board.is_empty(pos));
}

} // verus!
