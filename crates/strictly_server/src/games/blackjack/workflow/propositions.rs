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
/// This is the same type as [`strictly_blackjack::PayoutSettled`] — carrying
/// this token proves [`BankrollLedger::settle`] ran with a valid
/// [`BetDeducted`][strictly_blackjack::BetDeducted] proof.
///
/// Established by:
/// - [`execute_dealer_turn`][super::tools::execute_dealer_turn] on the normal play path
/// - [`execute_place_bet`][super::tools::execute_place_bet] on instant-finish paths
///   (player natural, dealer natural, both naturals) — settlement runs inside
///   [`GameBetting::place_bet`][crate::games::blackjack::GameBetting] before the
///   proof is returned
///
/// The compiler guarantees no double-deduction occurred: the `BetDeducted`
/// token is consumed (moved, not copied) exactly once by `settle`.
pub use strictly_blackjack::PayoutSettled;
