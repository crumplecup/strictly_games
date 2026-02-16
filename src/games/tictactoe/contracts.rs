//! Proof-carrying validation for tic-tac-toe using elicitation contracts.
//!
//! Instead of runtime-only validation, we use the elicitation framework's
//! contract system to carry proofs through the program. This enables:
//! - Zero-cost verification (proofs are PhantomData)
//! - Formally verified composition (Kani-checked)
//! - Type-enforced validation (can't execute without proof)

use super::action::{Move, MoveError};
use super::typestate::GameInProgress;
use elicitation::contracts::{And, Established, Prop, both};
use tracing::instrument;

// ─────────────────────────────────────────────────────────────
//  Propositions (Type-Level Statements)
// ─────────────────────────────────────────────────────────────

/// Proposition: The square at the move's position is empty.
pub struct SquareEmpty;
impl Prop for SquareEmpty {}

/// Proposition: It is the player's turn.
pub struct PlayerTurn;
impl Prop for PlayerTurn {}

/// Composite proposition: A move is legal (square empty AND player's turn).
pub type LegalMove = And<SquareEmpty, PlayerTurn>;

// ─────────────────────────────────────────────────────────────
//  Validation Functions (Establish Proofs)
// ─────────────────────────────────────────────────────────────

/// Validates that the square is empty.
///
/// Returns proof if valid, error otherwise.
#[instrument(skip(game))]
pub fn validate_square_empty(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<SquareEmpty>, MoveError> {
    if !game.board().is_empty(mov.position) {
        Err(MoveError::SquareOccupied(mov.position))
    } else {
        Ok(Established::assert())
    }
}

/// Validates that it's the player's turn.
///
/// Returns proof if valid, error otherwise.
#[instrument(skip(game))]
pub fn validate_player_turn(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<PlayerTurn>, MoveError> {
    if mov.player != game.to_move() {
        Err(MoveError::WrongPlayer(mov.player))
    } else {
        Ok(Established::assert())
    }
}

/// Validates all preconditions for a move.
///
/// Returns composite proof (SquareEmpty AND PlayerTurn) if valid.
/// This demonstrates proof composition via `both()`.
#[instrument(skip(game))]
pub fn validate_move(
    mov: &Move,
    game: &GameInProgress,
) -> Result<Established<LegalMove>, MoveError> {
    let square_proof = validate_square_empty(mov, game)?;
    let turn_proof = validate_player_turn(mov, game)?;
    Ok(both(square_proof, turn_proof))
}

// ─────────────────────────────────────────────────────────────
//  Proof-Carrying Execution
// ─────────────────────────────────────────────────────────────

/// Executes a move with proof that preconditions hold.
///
/// The `_proof` parameter is zero-cost (PhantomData) but enforces
/// that validation happened at compile time. Cannot call this
/// function without first obtaining proof via `validate_move()`.
#[instrument(skip(game, _proof))]
pub fn execute_move(
    mov: &Move,
    game: &mut GameInProgress,
    _proof: Established<LegalMove>,
) {
    // Proof guarantees: square empty AND player's turn
    // No need to revalidate - the type system enforces it
    game.board.set(mov.position, super::Square::Occupied(mov.player));
    game.history.push(*mov);
}
