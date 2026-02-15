//! First-class invariants for tic-tac-toe.
//!
//! Invariants are logical properties that must hold throughout game execution.
//! They are testable independently and serve as documentation of system guarantees.

#[cfg(kani)]
mod verification;

/// A logical property that must hold for a given state.
///
/// Invariants express system guarantees that should never be violated.
/// They are checked in debug builds and can be tested independently.
pub trait Invariant<S> {
    /// Checks if the invariant holds for the given state.
    fn holds(state: &S) -> bool;
    
    /// Human-readable description of the invariant.
    fn description() -> &'static str;
}

/// Violation of an invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation {
    /// Description of the violated invariant.
    pub description: String,
}

impl InvariantViolation {
    /// Creates a new invariant violation.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
        }
    }
}

/// A set of invariants that can be checked together.
///
/// This trait enables composition of multiple invariants into a single
/// verification step. Implementations are provided for tuples.
pub trait InvariantSet<S> {
    /// Checks all invariants in the set.
    ///
    /// Returns Ok(()) if all invariants hold, or Err with a list of
    /// violations if any invariant fails.
    fn check_all(state: &S) -> Result<(), Vec<InvariantViolation>>;
}

// Implement InvariantSet for 3-tuples
impl<S, I1, I2, I3> InvariantSet<S> for (I1, I2, I3)
where
    I1: Invariant<S>,
    I2: Invariant<S>,
    I3: Invariant<S>,
{
    fn check_all(state: &S) -> Result<(), Vec<InvariantViolation>> {
        let mut violations = Vec::new();
        
        if !I1::holds(state) {
            violations.push(InvariantViolation::new(I1::description()));
        }
        
        if !I2::holds(state) {
            violations.push(InvariantViolation::new(I2::description()));
        }
        
        if !I3::holds(state) {
            violations.push(InvariantViolation::new(I3::description()));
        }
        
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

// Implement InvariantSet for 2-tuples
impl<S, I1, I2> InvariantSet<S> for (I1, I2)
where
    I1: Invariant<S>,
    I2: Invariant<S>,
{
    fn check_all(state: &S) -> Result<(), Vec<InvariantViolation>> {
        let mut violations = Vec::new();
        
        if !I1::holds(state) {
            violations.push(InvariantViolation::new(I1::description()));
        }
        
        if !I2::holds(state) {
            violations.push(InvariantViolation::new(I2::description()));
        }
        
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

pub mod monotonic_board;
pub mod alternating_turn;
pub mod history_consistent;

pub use monotonic_board::MonotonicBoardInvariant;
pub use alternating_turn::AlternatingTurnInvariant;
pub use history_consistent::HistoryConsistentInvariant;

// Tic-tac-toe invariant set (all game invariants)
/// All tic-tac-toe invariants as a composable set.
pub type TicTacToeInvariants = (
    MonotonicBoardInvariant,
    AlternatingTurnInvariant,
    HistoryConsistentInvariant,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::tictactoe::{GameSetup, GameInProgress, GameResult, Move, Player, Position};

    #[test]
    fn test_invariant_set_holds_for_empty_game() {
        let game = GameSetup::new().start(Player::X);
        assert!(TicTacToeInvariants::check_all(&game).is_ok());
    }

    #[test]
    fn test_invariant_set_holds_after_moves() {
        let moves = vec![
            Move::new(Player::X, Position::TopLeft),
            Move::new(Player::O, Position::Center),
            Move::new(Player::X, Position::TopRight),
        ];
        
        if let Ok(GameResult::InProgress(game)) = GameInProgress::replay(&moves) {
            assert!(TicTacToeInvariants::check_all(&game).is_ok());
        } else {
            panic!("Expected in-progress game");
        }
    }

    #[test]
    fn test_invariant_set_detects_violations() {
        let game = GameSetup::new().start(Player::X);
        let action = Move::new(Player::X, Position::Center);
        
        if let Ok(GameResult::InProgress(mut game)) = game.make_move(action) {
            // Corrupt the board
            game.board.set(Position::TopLeft, crate::games::tictactoe::Square::Occupied(Player::O));
            
            // Should detect violation
            let result = TicTacToeInvariants::check_all(&game);
            assert!(result.is_err());
            
            let violations = result.unwrap_err();
            assert!(!violations.is_empty());
        }
    }

    #[test]
    fn test_two_invariants_as_set() {
        let game = GameSetup::new().start(Player::X);
        
        type TwoInvariants = (MonotonicBoardInvariant, AlternatingTurnInvariant);
        assert!(TwoInvariants::check_all(&game).is_ok());
    }
}

