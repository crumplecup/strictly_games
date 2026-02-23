//! Verus proofs for tic-tac-toe game invariants.
//!
//! ⚠️ **MIRROR PATTERN**: Types are duplicated from `strictly_tictactoe` crate.
//! 
//! **Maintenance Contract:**
//! - When `Player`, `Position`, `Square`, or `Board` change in strictly_tictactoe,
//!   the mirror definitions below MUST be updated manually.
//! - Enum variants must match exactly
//! - Method signatures must be equivalent (usize ↔ int, &self ↔ self)
//! - Invariants and properties must align
//!
//! **Why this pattern:**
//! Verus cannot resolve workspace dependencies when run with `verus --crate-type=lib`.
//! This mirrors elicitation's `elicitation_verus` crate approach.
//!
//! **Detection:**
//! TODO: Add static check comparing type structures (count variants, method names).
//! For now: Run `just verify-verus-tracked` after ANY strictly_tictactoe changes.

use verus_builtin::*;
use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

// ============================================================================
// MIRRORED TYPE DEFINITIONS
// Source: strictly_tictactoe/src/{types.rs, position.rs}
// Last synced: 2026-02-23
// ============================================================================

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

// ============================================================================
// VERIFICATION PROOFS
// ============================================================================

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
