//! Display mode enum for blackjack.

use elicitation::{Elicit, KaniCompose};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Display strategies for a [`BlackjackState`](crate::vsm::BlackjackState).
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
pub enum BlackjackDisplayMode {
    /// Show the table with all hands and available actions.
    #[default]
    Table,
    /// Show the scorecard and round history.
    Scorecard,
}
