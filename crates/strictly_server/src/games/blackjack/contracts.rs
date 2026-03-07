//! Proof-carrying validation for blackjack using elicitation contracts.
//!
//! Instead of runtime-only validation, we use the elicitation framework's
//! contract system to carry proofs through the program.

use super::action::{ActionError, BasicAction, PlayerAction};
use super::typestate::GamePlayerTurn;
use elicitation::contracts::{And, Established, Prop, both};
use tracing::instrument;

// ─────────────────────────────────────────────────────────────
//  Propositions (Type-Level Statements)
// ─────────────────────────────────────────────────────────────

/// Proposition: The action is valid for the current game state.
pub struct ValidAction;
impl Prop for ValidAction {}

/// Proposition: The hand is not bust.
pub struct NotBust;
impl Prop for NotBust {}

/// Composite proposition: An action is legal (valid AND not bust).
pub type LegalAction = And<ValidAction, NotBust>;

// ─────────────────────────────────────────────────────────────
//  Validation Functions (Establish Proofs)
// ─────────────────────────────────────────────────────────────

/// Validates that the action is valid for the current state.
#[instrument(skip(game))]
pub fn validate_valid_action(
    action: &PlayerAction,
    game: &GamePlayerTurn,
) -> Result<Established<ValidAction>, ActionError> {
    // Validate hand index
    if action.hand_index() >= game.player_hands.len() {
        return Err(ActionError::InvalidHandIndex(action.hand_index()));
    }

    // Validate hand index matches current
    if action.hand_index() != game.current_hand_index {
        return Err(ActionError::InvalidHandIndex(action.hand_index()));
    }

    // All basic actions (Hit/Stand) are always valid if hand index is correct
    Ok(Established::assert())
}

/// Validates that the hand is not bust.
#[instrument(skip(game))]
pub fn validate_not_bust(
    action: &PlayerAction,
    game: &GamePlayerTurn,
) -> Result<Established<NotBust>, ActionError> {
    let hand = &game.player_hands[action.hand_index()];

    if hand.is_bust() {
        Err(ActionError::HandBust)
    } else {
        Ok(Established::assert())
    }
}

/// Validates all preconditions for an action.
///
/// Returns composite proof (ValidAction AND NotBust) if valid.
#[instrument(skip(game))]
pub fn validate_action(
    action: &PlayerAction,
    game: &GamePlayerTurn,
) -> Result<Established<LegalAction>, ActionError> {
    let valid_proof = validate_valid_action(action, game)?;
    let bust_proof = validate_not_bust(action, game)?;
    Ok(both(valid_proof, bust_proof))
}

// ─────────────────────────────────────────────────────────────
//  Proof-Carrying Execution
// ─────────────────────────────────────────────────────────────

/// Executes an action with proof that preconditions hold.
///
/// The `_proof` parameter is zero-cost (PhantomData) but enforces
/// that validation happened at compile time.
#[instrument(skip(game, _proof))]
pub fn execute_action(
    action: &PlayerAction,
    game: &mut GamePlayerTurn,
    _proof: Established<LegalAction>,
) -> Result<(), ActionError> {
    match action.action() {
        BasicAction::Hit => {
            // Deal card to current hand
            if let Some(card) = game.deck.deal() {
                game.player_hands[action.hand_index()].add_card(card);
                Ok(())
            } else {
                Err(ActionError::DeckExhausted)
            }
        }
        BasicAction::Stand => {
            // No state change - hand is marked complete by caller
            Ok(())
        }
    }
}
