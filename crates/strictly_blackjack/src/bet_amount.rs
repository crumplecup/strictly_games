//! Validated bet amount newtype for elicitation.
//!
//! [`BetAmount`] wraps a `u64` bet. At runtime it is produced either by the
//! `ContextualFactory`-based tool path (bounded by the player's bankroll) or,
//! under the `shuffle` feature, by [`BetAmount::random_generator`] for
//! simulation and testing.

use elicitation::Elicit;
use serde::{Deserialize, Serialize};

/// A validated bet amount in chips.
///
/// Produced by the betting elicitation tools. The inner value is guaranteed
/// positive; range enforcement (≤ bankroll) is the factory's responsibility.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Elicit,
    schemars::JsonSchema,
)]
#[cfg_attr(feature = "shuffle", derive(elicitation_derive::Rand))]
#[cfg_attr(feature = "shuffle", rand(bounded(1, 10_001)))]
pub struct BetAmount(pub u64);

impl BetAmount {
    /// Unwraps to the raw chip count.
    pub fn amount(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for BetAmount {
    #[tracing::instrument(skip(self, f))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
