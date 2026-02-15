//! Contract-based validation for tic-tac-toe.
//!
//! Contracts are declarative rules that can be checked independently.
//! They serve as both validation and documentation of game rules.

use super::action::{Move, MoveError};
use super::phases::InProgress;
use super::typestate::Game;
use super::{Board, Player, Position};
use tracing::{instrument, warn};

// ─────────────────────────────────────────────────────────────
//  Game Invariants
// ─────────────────────────────────────────────────────────────

/// Invariant: Board state is consistent (X's and O's differ by ≤ 1).
pub struct BoardConsistent;

impl BoardConsistent {
    #[instrument(skip(board))]
    pub fn holds(board: &Board) -> bool {
        let x_count = board.squares().iter().filter(|s| matches!(s, super::Square::Occupied(Player::X))).count();
        let o_count = board.squares().iter().filter(|s| matches!(s, super::Square::Occupied(Player::O))).count();
        
        let diff = if x_count >= o_count {
            x_count - o_count
        } else {
            o_count - x_count
        };
        
        let valid = diff <= 1;
        if !valid {
            warn!(x_count, o_count, "Board consistency violated");
        }
        valid
    }
}

/// Invariant: History length matches filled squares.
pub struct HistoryComplete;

impl HistoryComplete {
    #[instrument(skip(game))]
    pub fn holds(game: &Game<InProgress>) -> bool {
        let filled = game.board().squares().iter().filter(|s| !matches!(s, super::Square::Empty)).count();
        let history_len = game.history().len();
        
        let valid = filled == history_len;
        if !valid {
            warn!(filled, history_len, "History completeness violated");
        }
        valid
    }
}

// ─────────────────────────────────────────────────────────────
//  Move Preconditions
// ─────────────────────────────────────────────────────────────

/// Precondition: The square at the move's position must be empty.
pub struct SquareIsEmpty;

impl SquareIsEmpty {
    #[instrument(skip(game))]
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        if !game.board().is_empty(mov.position) {
            Err(MoveError::SquareOccupied(mov.position))
        } else {
            Ok(())
        }
    }
}

/// Precondition: It must be the player's turn.
pub struct PlayersTurn;

impl PlayersTurn {
    #[instrument(skip(game))]
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        if mov.player != game.to_move() {
            Err(MoveError::WrongPlayer(mov.player))
        } else {
            Ok(())
        }
    }
}

/// Composite precondition: A move is legal if the square is empty and it's the player's turn.
pub struct LegalMove;

impl LegalMove {
    /// Validates all preconditions for a move.
    #[instrument(skip(game))]
    pub fn check(mov: &Move, game: &Game<InProgress>) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;
        PlayersTurn::check(mov, game)?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────
//  Invariant Assertions
// ─────────────────────────────────────────────────────────────

/// Asserts that all game invariants hold (panic on violation in debug builds).
#[instrument(skip(game))]
pub fn assert_invariants(game: &Game<InProgress>) {
    debug_assert!(BoardConsistent::holds(game.board()), "Board consistency violated");
    debug_assert!(HistoryComplete::holds(game), "History completeness violated");
}
