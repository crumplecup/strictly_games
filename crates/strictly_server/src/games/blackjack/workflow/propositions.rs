//! Proof-carrying propositions for the blackjack workflow.
//!
//! Each proposition is a zero-cost `PhantomData` marker that the type system
//! uses to enforce legal phase transitions at compile time.
//!
//! # Contract chain
//!
//! ```text
//! True → (PlaceBetTool) → BetPlaced → (PlayActionTool) → PlayerTurnComplete
//!                                ↑                              |
//!                          (loop while PlayerTurn)             ↓
//!                                         (DealerTurnTool) → PayoutSettled
//! ```
//!
//! Financial sub-chain (inside the typestate layer):
//!
//! ```text
//! BankrollLedger::debit() → BetDeducted → BankrollLedger::settle() → PayoutSettled
//! ```

use elicitation::contracts::Prop;

/// Proposition: a bet has been placed and initial cards dealt.
///
/// Established by [`execute_place_bet`][super::tools::execute_place_bet].
/// Required by [`execute_play_action`][super::tools::execute_play_action].
pub struct BetPlaced;
impl Prop for BetPlaced {}

/// Proposition: the player's turn is complete (stood, bust, or blackjack).
///
/// Established by [`execute_play_action`][super::tools::execute_play_action]
/// when the hand reaches a terminal player state.
/// Required by [`execute_dealer_turn`][super::tools::execute_dealer_turn].
pub struct PlayerTurnComplete;
impl Prop for PlayerTurnComplete {}

/// Proposition: the hand's payout has been correctly settled.
///
/// Established by [`execute_dealer_turn`][super::tools::execute_dealer_turn]
/// and by instant-finish paths in
/// [`execute_place_bet`][super::tools::execute_place_bet].
///
/// Carrying this token is proof that [`BankrollLedger::settle`] ran with
/// a valid [`BetDeducted`][crate::games::blackjack::ledger::BetDeducted]
/// proof — the compiler guarantees no double-deduction occurred.
pub struct PayoutSettled;
impl Prop for PayoutSettled {}
