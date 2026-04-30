//! Display mode enum for craps.

use elicitation::{Elicit, KaniCompose};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Display strategies for a [`CrapsState`](crate::vsm::CrapsState).
#[cfg_attr(kani, derive(kani::Arbitrary))]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
    KaniCompose,
)]
pub enum CrapsDisplayMode {
    /// Show the table with bets and dice.
    #[default]
    Table,
    /// Show round statistics and payout history.
    Stats,
}
