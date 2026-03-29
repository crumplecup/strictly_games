//! Proof-carrying propositions for the craps workflow.
//!
//! Each proposition is a zero-cost marker that the type system uses to
//! enforce legal phase transitions at compile time.
//!
//! # Contract chain
//!
//! ```text
//! True → (execute_place_bets) → BetsPlaced
//!     → (execute_comeout_roll) → PointEstablished | RoundSettled
//!     → (execute_point_roll loop) → PointEstablished | RoundSettled
//! ```

use elicitation::VerifiedWorkflow;

/// Proposition: bets have been placed and validated against bankrolls.
///
/// Established by [`super::tools::execute_place_bets`].
/// Required by [`super::tools::execute_comeout_roll`].
#[derive(elicitation::Prop)]
pub struct BetsPlaced;
impl VerifiedWorkflow for BetsPlaced {}

/// Proposition: a point has been established on the come-out roll.
///
/// Established by [`super::tools::execute_comeout_roll`] on point values.
/// Required by [`super::tools::execute_point_roll`].
/// Recycled on non-resolving point-phase rolls.
#[derive(elicitation::Prop)]
pub struct PointEstablished;
impl VerifiedWorkflow for PointEstablished {}
