//! Contract-based validation for tic-tac-toe.
//!
//! Contracts define correctness through preconditions and postconditions.
//! They formalize the Hoare-style reasoning: {P} action {Q}

use super::action::{Move, MoveError};
use super::invariants::{InvariantSet, TicTacToeInvariants};
use super::typestate::GameInProgress;
use super::{Board, Player};
use tracing::{instrument, warn};

// ─────────────────────────────────────────────────────────────
//  Contract Trait
// ─────────────────────────────────────────────────────────────

/// A contract defines preconditions and postconditions for state transitions.
///
/// Contracts formalize Hoare-style reasoning:
/// - Precondition: {P(state, action)} - must hold before applying action
/// - Postcondition: {Q(before, after)} - must hold after applying action
pub trait Contract<S, A> {
    /// Checks preconditions before applying the action.
    fn pre(state: &S, action: &A) -> Result<(), MoveError>;
    
    /// Checks postconditions after applying the action.
    ///
    /// This verifies that the transition maintained system invariants.
    fn post(before: &S, after: &S) -> Result<(), MoveError>;
}

// ─────────────────────────────────────────────────────────────
//  Move Preconditions
// ─────────────────────────────────────────────────────────────

/// Precondition: The square at the move's position must be empty.
pub struct SquareIsEmpty;

impl SquareIsEmpty {
    #[instrument(skip(game))]
    pub fn check(mov: &Move, game: &GameInProgress) -> Result<(), MoveError> {
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
    pub fn check(mov: &Move, game: &GameInProgress) -> Result<(), MoveError> {
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
    pub fn check(mov: &Move, game: &GameInProgress) -> Result<(), MoveError> {
        SquareIsEmpty::check(mov, game)?;
        PlayersTurn::check(mov, game)?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────
//  Move Contract (Pre + Post)
// ─────────────────────────────────────────────────────────────

/// Contract for move actions.
///
/// Preconditions:
/// - Square must be empty
/// - Must be player's turn
///
/// Postconditions:
/// - Board remains monotonic
/// - Players still alternate
/// - History remains consistent with board
pub struct MoveContract;

impl Contract<GameInProgress, Move> for MoveContract {
    fn pre(game: &GameInProgress, action: &Move) -> Result<(), MoveError> {
        LegalMove::check(action, game)
    }
    
    fn post(_before: &GameInProgress, after: &GameInProgress) -> Result<(), MoveError> {
        // Verify all invariants using the composed set
        TicTacToeInvariants::check_all(after).map_err(|violations| {
            let descriptions = violations
                .iter()
                .map(|v| v.description.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            MoveError::InvariantViolation(format!("Postcondition failed: {}", descriptions))
        })
    }
}

// ─────────────────────────────────────────────────────────────
//  Legacy Invariants (for backward compatibility)
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
    pub fn holds(game: &GameInProgress) -> bool {
        let filled = game.board().squares().iter().filter(|s| !matches!(s, super::Square::Empty)).count();
        let history_len = game.history().len();
        
        let valid = filled == history_len;
        if !valid {
            warn!(filled, history_len, "History completeness violated");
        }
        valid
    }
}

/// Asserts that all game invariants hold (panic on violation in debug builds).
#[instrument(skip(game))]
pub fn assert_invariants(game: &GameInProgress) {
    debug_assert!(BoardConsistent::holds(game.board()), "Board consistency violated");
    debug_assert!(HistoryComplete::holds(game), "History completeness violated");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::{GameSetup, GameResult, Position};

    #[test]
    fn test_precondition_empty_square() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        // Should pass - square is empty
        assert!(MoveContract::pre(&game, &action).is_ok());
    }

    #[test]
    fn test_precondition_occupied_square() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(game)) = game.make_move(action) {
            // Try to play same square
            let action2 = Move::new(Player::O, Position::Center);
            assert!(matches!(
                MoveContract::pre(&game, &action2),
                Err(MoveError::SquareOccupied(_))
            ));
        }
    }

    #[test]
    fn test_precondition_wrong_turn() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::O, Position::Center);  // O plays when it's X's turn
        
        assert!(matches!(
            MoveContract::pre(&game, &action),
            Err(MoveError::WrongPlayer(_))
        ));
    }

    #[test]
    fn test_postcondition_holds_after_move() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(after)) = game.clone().make_move(action) {
            // Postcondition should hold
            assert!(MoveContract::post(&game, &after).is_ok());
        }
    }

    #[test]
    fn test_postcondition_detects_corruption() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(mut after)) = game.clone().make_move(action) {
            // Corrupt the board
            after.board.set(Position::TopLeft, super::super::Square::Occupied(Player::O));
            
            // Postcondition should fail
            assert!(MoveContract::post(&game, &after).is_err());
        }
    }
}
