//! Proof-carrying validation for blackjack using elicitation contracts.
//!
//! Instead of runtime-only validation, we use the elicitation framework's
//! contract system to carry proofs through the program.

use elicitation::Generator;
use elicitation::VerifiedWorkflow;
use elicitation::contracts::{And, Established, ProvableFrom, both};
#[cfg(not(kani))]
use tracing::instrument;

use crate::{ActionError, BasicAction, GamePlayerTurn, PlayerAction};

// ─────────────────────────────────────────────────────────────
//  Bankroll Proposition
// ─────────────────────────────────────────────────────────────

/// Proposition: the initial bankroll is positive (greater than zero).
///
/// Obtaining this token is the only way to call `bj_start_betting`.
/// The issuing function [`validate_bankroll_positive`] is the "unsafe block"
/// that upholds the invariant: it checks `amount > 0` before asserting.
#[derive(elicitation::Prop)]
pub struct BankrollPositive;
impl VerifiedWorkflow for BankrollPositive {}

/// Validates that a bankroll amount is positive and issues a proof token.
///
/// This is the single point of trust for the `BankrollPositive` invariant.
/// Only code that holds `Established<BankrollPositive>` may start the betting phase.
#[cfg_attr(not(kani), instrument)]
pub fn validate_bankroll_positive(
    amount: u64,
) -> Result<Established<BankrollPositive>, ActionError> {
    if amount > 0 {
        Ok(Established::assert())
    } else {
        Err(ActionError::ZeroBankroll)
    }
}

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
#[cfg_attr(not(kani), instrument(skip(game)))]
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
#[cfg_attr(not(kani), instrument(skip(game)))]
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
#[cfg_attr(not(kani), instrument(skip(game)))]
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
#[cfg_attr(not(kani), instrument(skip(game, _proof)))]
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
#[prop(
    credential = BlackjackRulesEvidence,
    kani_invariant_fn = "blackjack_consistent",
    creusot_invariant_fn = "blackjack_consistent",
    creusot_inv_body = "match state { BlackjackState::Setup { .. } => true, BlackjackState::Betting { inner, .. } => inner.bankroll@ > 0, BlackjackState::PlayerTurn { inner, .. } => inner.current_hand_index@ < inner.num_hands@, BlackjackState::DealerTurn { .. } => true, BlackjackState::Finished { inner, .. } => inner.num_hands@ <= MAX_PLAYER_HANDS@, }",
    verus_invariant_fn = "blackjack_consistent",
    verus_inv_body = "match *state { BlackjackState::Betting { bankroll, .. } => bankroll@ > 0, BlackjackState::PlayerTurn { current_hand_index, num_hands, .. } => current_hand_index@ < num_hands@, BlackjackState::Finished { num_hands, .. } => num_hands@ <= 4, _ => true, }",
    verus_state_body = "Betting { bankroll: u64 }, PlayerTurn { current_hand_index: usize, num_hands: usize }, Finished { num_hands: usize }, _Other,"
)]
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

/// Evidence bundle for establishing [`BlackjackConsistent`] in the `Betting` state.
///
/// Assembling this forces proof that the initial bankroll is positive,
/// which is exactly the invariant for `Betting { inner } => inner.bankroll@ > 0`.
pub struct BettingStateEvidence {
    /// Proof that the initial bankroll is positive.
    pub bankroll_positive: Established<BankrollPositive>,
}

impl ProvableFrom<BettingStateEvidence> for BlackjackConsistent {}

#[cfg(kani)]
impl kani::Arbitrary for BlackjackRulesEvidence {
    fn any() -> Self {
        Self {
            valid_action: Established::assert(),
            not_bust: Established::assert(),
        }
    }
}
