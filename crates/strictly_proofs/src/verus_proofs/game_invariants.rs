//! Verus proofs for tic-tac-toe game invariants.
//!
//! Types are redefined here for Verus verification (Verus can't resolve workspace deps).

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

// Type definitions (mirrored from strictly_tictactoe)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    X,
    O,
}

impl Player {
    pub open spec fn opponent(self) -> Self {
        match self {
            Player::X => Player::O,
            Player::O => Player::X,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Position {
    pub open spec fn to_index(self) -> int {
        match self {
            Position::TopLeft => 0,
            Position::TopCenter => 1,
            Position::TopRight => 2,
            Position::MiddleLeft => 3,
            Position::Center => 4,
            Position::MiddleRight => 5,
            Position::BottomLeft => 6,
            Position::BottomCenter => 7,
            Position::BottomRight => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Square {
    Empty,
    Occupied(Player),
}

pub struct Board {
    pub squares: Seq<Square>,
}

impl Board {
    pub open spec fn new() -> Self {
        Board {
            squares: Seq::new(9, |_i: int| Square::Empty),
        }
    }

    pub open spec fn get(self, pos: Position) -> Square {
        self.squares[pos.to_index()]
    }

    pub open spec fn is_empty(self, pos: Position) -> bool {
        self.get(pos) == Square::Empty
    }
}

// Proofs

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
