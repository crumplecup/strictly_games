//! Proof-carrying propositions for the blackjack workflow.
//!
//! Each proposition is a zero-cost `PhantomData` marker that the type system
//! uses to enforce legal phase transitions at compile time.
//!
//! # Contract chain
//!
//! ```text
//! True → (execute_place_bet) → BetPlaced → (execute_play_action loop) → PlayerTurnComplete
//!                                                                               ↓
//!                                                    (execute_dealer_turn) → PayoutSettled
//! ```
//!
//! Financial sub-chain (inside the typestate layer):
//!
//! ```text
//! BankrollLedger::debit() → BetDeducted → BankrollLedger::settle() → PayoutSettled
//! ```

use elicitation::VerifiedWorkflow;

/// Proposition: a bet has been placed and initial cards dealt.
///
/// Established by [`crate::execute_place_bet`].
/// Required by [`crate::execute_play_action`].
#[derive(elicitation::Prop)]
pub struct BetPlaced;
impl VerifiedWorkflow for BetPlaced {}

/// Proposition: the player's turn is complete (stood, bust, or blackjack).
///
/// Established by [`crate::execute_play_action`] when the hand reaches a
/// terminal player state. Required by [`crate::execute_dealer_turn`].
#[derive(elicitation::Prop)]
pub struct PlayerTurnComplete;
impl VerifiedWorkflow for PlayerTurnComplete {}
