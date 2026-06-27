//! Step 1–5: `IceCreamFlavor` — from bare enum through `#[derive(Elicit)]`, prompts, and styles.

use elicitation::Elicit;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A flavor of ice cream.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Elicit,
)]
#[prompt("Pick a flavor:")]
pub enum IceCreamFlavor {
    /// Classic vanilla bean.
    Vanilla,
    /// Rich dark chocolate.
    Chocolate,
    /// Fresh strawberry.
    Strawberry,
}

impl std::fmt::Display for IceCreamFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vanilla => write!(f, "Vanilla"),
            Self::Chocolate => write!(f, "Chocolate"),
            Self::Strawberry => write!(f, "Strawberry"),
        }
    }
}
