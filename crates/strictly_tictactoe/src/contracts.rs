//! Proof-carrying validation for tic-tac-toe using elicitation contracts.
//!
//! Instead of runtime-only validation, we use the elicitation framework's
//! contract system to carry proofs through the program. This enables:
//! - Zero-cost verification (proofs are PhantomData)
//! - Formally verified composition (Kani-checked)
//! - Type-enforced validation (can't execute without proof)

use crate::action::{Move, MoveError};
use crate::typestate::GameInProgress;
use elicitation::VerifiedWorkflow;
use elicitation::contracts::{And, Established, ProvableFrom, both};
#[cfg(not(kani))]
use tracing::instrument;

// ─────────────────────────────────────────────────────────────
//  Propositions (Type-Level Statements)
// ─────────────────────────────────────────────────────────────

/// Proposition: The square at the move's position is empty.
#[derive(elicitation::Prop)]
pub struct SquareEmpty;
impl VerifiedWorkflow for SquareEmpty {}

/// Proposition: It is the player's turn.
#[derive(elicitation::Prop)]
pub struct PlayerTurn;
impl VerifiedWorkflow for PlayerTurn {}

/// Composite proposition: A move is legal (square empty AND player's turn).
/// `And<SquareEmpty, PlayerTurn>: VerifiedWorkflow` via blanket impl — proof composition is automatic.
pub type LegalMove = And<SquareEmpty, PlayerTurn>;

// ─────────────────────────────────────────────────────────────
//  Validation Functions (Establish Proofs)
// ─────────────────────────────────────────────────────────────

/// Validates that the square is empty.
///
/// Returns proof if valid, error otherwise.
///
/// **Contract**: `Ok(_)` iff `game.board().is_empty(mov.position)`.
#[cfg_attr(kani, kani::ensures(|result| result.is_ok() == game.board().is_empty(mov.position)))]
#[cfg_attr(not(kani), instrument(skip(game)))]
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
///
/// **Contract**: `Ok(_)` iff `mov.player == game.to_move()`.
#[cfg_attr(kani, kani::ensures(|result| result.is_ok() == (mov.player == game.to_move())))]
#[cfg_attr(not(kani), instrument(skip(game)))]
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
///
/// **Contract**: `Ok(_)` iff the square is empty AND it is the player's turn.
#[cfg_attr(kani, kani::ensures(|result| {
    result.is_ok() == (game.board().is_empty(mov.position) && mov.player == game.to_move())
}))]
#[cfg_attr(not(kani), instrument(skip(game)))]
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
///
/// **Requires**: the square at `mov.position` is empty AND it is `mov.player`'s turn.
/// **Ensures**: the square is set to `Occupied(mov.player)` and `mov` is the last
///   history entry.  All other state (`to_move`, other squares) is automatically
///   preserved by Kani's write-set inference.
#[cfg_attr(kani, kani::requires(game.board().is_empty(mov.position)))]
#[cfg_attr(kani, kani::requires(mov.player == game.to_move()))]
#[cfg_attr(kani, kani::ensures(|_| game.board().get(mov.position) == crate::Square::Occupied(mov.player)))]
#[cfg_attr(kani, kani::ensures(|_| game.history().last() == Some(mov)))]
#[cfg_attr(not(kani), instrument(skip(game, _proof)))]
pub fn execute_move(mov: &Move, game: &mut GameInProgress, _proof: Established<LegalMove>) {
    // Proof guarantees: square empty AND player's turn
    // No need to revalidate - the type system enforces it
    game.board
        .set(mov.position, crate::Square::Occupied(mov.player));
    game.history.push(*mov);
}

// ─────────────────────────────────────────────────────────────
//  Top-Level Invariant
// ─────────────────────────────────────────────────────────────

/// Proposition: the game is being played according to the rules.
///
/// Wired to [`TicTacToeRulesEvidence`]: formal-method harnesses call
/// `Established::prove(&TicTacToeConsistent::kani_proof_credential())`.
#[derive(elicitation::Prop)]
#[prop(credential = TicTacToeRulesEvidence)]
pub struct TicTacToeConsistent;

impl VerifiedWorkflow for TicTacToeConsistent {}

/// Evidence bundle for establishing [`TicTacToeConsistent`].
///
/// Assembling this forces proof of both move preconditions — the square was
/// empty and it was the player's turn — before the rules invariant can be issued.
pub struct TicTacToeRulesEvidence {
    /// Proof that the target square was empty before the move.
    pub square_empty: Established<SquareEmpty>,
    /// Proof that it was the moving player's turn.
    pub player_turn: Established<PlayerTurn>,
}

impl ProvableFrom<TicTacToeRulesEvidence> for TicTacToeConsistent {}

#[cfg(kani)]
impl kani::Arbitrary for TicTacToeRulesEvidence {
    fn any() -> Self {
        Self {
            square_empty: Established::assert(),
            player_turn: Established::assert(),
        }
    }
}
