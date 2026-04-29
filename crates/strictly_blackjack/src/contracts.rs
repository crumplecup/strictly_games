//! Proof-carrying validation for blackjack using elicitation contracts.
//!
//! Instead of runtime-only validation, we use the elicitation framework's
//! contract system to carry proofs through the program.

use elicitation::Generator;
use elicitation::VerifiedWorkflow;
use elicitation::contracts::{And, Established, ProvableFrom, both};
use tracing::instrument;

use crate::{ActionError, BasicAction, GamePlayerTurn, PlayerAction};

// ─────────────────────────────────────────────────────────────
//  Propositions (Type-Level Statements)
// ─────────────────────────────────────────────────────────────

/// Proposition: the action is valid for the current game state.
#[derive(elicitation::Prop)]
pub struct ValidAction;
impl VerifiedWorkflow for ValidAction {}

/// Proposition: the hand is not bust.
#[derive(elicitation::Prop)]
pub struct NotBust;
impl VerifiedWorkflow for NotBust {}

/// Composite proposition: an action is legal (valid AND not bust).
/// `And<ValidAction, NotBust>: VerifiedWorkflow` via blanket impl — proof composition is automatic.
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
    if action.hand_index() >= game.player_hands.len() {
        return Err(ActionError::InvalidHandIndex(action.hand_index()));
    }
    if action.hand_index() != game.current_hand_index {
        return Err(ActionError::WrongHandTurn {
            expected: game.current_hand_index,
            got: action.hand_index(),
        });
    }
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
            if let Some(card) = game.shoe.generate() {
                game.player_hands[action.hand_index()].add_card(card);
                Ok(())
            } else {
                Err(ActionError::DeckExhausted)
            }
        }
        BasicAction::Stand => Ok(()),
    }
}

// ─────────────────────────────────────────────────────────────
//  Top-Level Invariant
// ─────────────────────────────────────────────────────────────

/// Proposition: the game is being played according to blackjack rules.
///
/// Wired to [`BlackjackRulesEvidence`]: formal-method harnesses call
/// `Established::prove(&BlackjackConsistent::kani_proof_credential())`.
#[derive(elicitation::Prop)]
#[prop(credential = BlackjackRulesEvidence)]
pub struct BlackjackConsistent;

impl VerifiedWorkflow for BlackjackConsistent {}

/// Evidence bundle for establishing [`BlackjackConsistent`].
///
/// Assembling this forces proof that:
/// - the action targets a valid hand index and is the player's turn, and
/// - the targeted hand has not already busted.
///
/// These are the two preconditions blackjack rules enforce per player action.
pub struct BlackjackRulesEvidence {
    /// Proof that the action targets a valid hand and it is the player's turn.
    pub valid_action: Established<ValidAction>,
    /// Proof that the targeted hand is not bust.
    pub not_bust: Established<NotBust>,
}

impl ProvableFrom<BlackjackRulesEvidence> for BlackjackConsistent {}

#[cfg(kani)]
impl kani::Arbitrary for BlackjackRulesEvidence {
    fn any() -> Self {
        Self {
            valid_action: Established::assert(),
            not_bust: Established::assert(),
        }
    }
}
