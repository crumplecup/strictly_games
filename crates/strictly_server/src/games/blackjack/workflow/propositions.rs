//! Proof-carrying propositions for the blackjack workflow.
//!
//! Each proposition is a zero-cost `PhantomData` marker that the type system
//! uses to enforce legal phase transitions at compile time.
//!
//! # Contract chain
//!
//! ```text
//! True → (PlaceBetTool) → BetPlaced → (PlayActionTool) → HandComplete
//!                                ↑                              |
//!                          (loop while PlayerTurn)             ↓
//!                                         (DealerTurnTool) → HandResolved
//! ```

use elicitation::contracts::Prop;

/// Proposition: a bet has been placed and initial cards dealt.
///
/// Established by [`PlaceBetTool`][super::tools::PlaceBetTool].
/// Required by [`PlayActionTool`][super::tools::PlayActionTool].
pub struct BetPlaced;
impl Prop for BetPlaced {}

/// Proposition: the player's turn is complete (stood, bust, or blackjack).
///
/// Established by [`PlayActionTool`][super::tools::PlayActionTool] when the
/// hand reaches a terminal player state.
/// Required by [`DealerTurnTool`][super::tools::DealerTurnTool].
pub struct PlayerTurnComplete;
impl Prop for PlayerTurnComplete {}

/// Proposition: the dealer has played and outcomes are resolved.
///
/// Established by [`DealerTurnTool`][super::tools::DealerTurnTool].
pub struct HandResolved;
impl Prop for HandResolved {}
