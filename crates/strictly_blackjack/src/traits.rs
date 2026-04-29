//! Trait interfaces for blackjack rule enforcement.
//!
//! These traits define the canonical, formally-sanctioned routes for
//! construction and validation that honour `ProvableFrom`.  Implementors
//! must produce genuine `Established` tokens — only the real validation
//! functions (backed by the game state) may issue them.

use crate::action::PlayerAction;
use crate::contracts::{NotBust, ValidAction};
use crate::error::ActionError;
use crate::typestate::GamePlayerTurn;
use elicitation::contracts::Established;

/// Canonical interface for enforcing blackjack action rules.
///
/// Implementors validate player-action preconditions against the live game
/// state and return zero-cost proof tokens.  The proof tokens are later
/// assembled into a [`crate::contracts::BlackjackRulesEvidence`] bundle that
/// establishes [`crate::contracts::BlackjackConsistent`].
pub trait BlackjackRuleEnforcer: Send + Sync {
    /// Verify that the action targets the correct hand and is the player's turn.
    ///
    /// Returns `Established<ValidAction>` if the hand index is in range and
    /// it is the player's turn, otherwise an error.
    fn verify_valid_action(
        &self,
        action: &PlayerAction,
        game: &GamePlayerTurn,
    ) -> Result<Established<ValidAction>, ActionError>;

    /// Verify that the targeted hand is not bust.
    ///
    /// Returns `Established<NotBust>` if the current hand value does not
    /// exceed 21, otherwise an error.
    fn verify_not_bust(
        &self,
        action: &PlayerAction,
        game: &GamePlayerTurn,
    ) -> Result<Established<NotBust>, ActionError>;
}
