//! Trait interfaces for tic-tac-toe rule enforcement.
//!
//! These traits define the canonical, formally-sanctioned routes for
//! construction and validation that honour `ProvableFrom`.  Implementors
//! must produce genuine `Established` tokens — only the real validation
//! functions (backed by the game state) may issue them.

use crate::action::{Move, MoveError};
use crate::contracts::{PlayerTurn, SquareEmpty};
use crate::typestate::GameInProgress;
use elicitation::contracts::Established;

/// Canonical interface for enforcing tic-tac-toe move rules.
///
/// Implementors validate move preconditions against the live game state and
/// return zero-cost proof tokens.  The proof tokens are later assembled into
/// a [`crate::contracts::TicTacToeRulesEvidence`] bundle that establishes
/// [`crate::contracts::TicTacToeConsistent`].
pub trait TicTacToeRuleEnforcer: Send + Sync {
    /// Verify that the target square is empty.
    ///
    /// Returns `Established<SquareEmpty>` if the square at `mov.position`
    /// is unoccupied, otherwise an error.
    fn verify_square_empty(
        &self,
        mov: &Move,
        game: &GameInProgress,
    ) -> Result<Established<SquareEmpty>, MoveError>;

    /// Verify that it is the player's turn.
    ///
    /// Returns `Established<PlayerTurn>` if `mov.player == game.to_move()`,
    /// otherwise an error.
    fn verify_player_turn(
        &self,
        mov: &Move,
        game: &GameInProgress,
    ) -> Result<Established<PlayerTurn>, MoveError>;
}
