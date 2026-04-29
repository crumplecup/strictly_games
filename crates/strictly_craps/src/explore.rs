//! Agent exploration actions for craps.
//!
//! [`CrapsAction`] combines commit actions (PlaceBet, Done) with explore
//! actions (ViewPoint, ViewActiveBets, etc.) in a single [`Select`] enum.
//! The [`Filter`](elicitation::Filter) trait gates which options surface
//! per participant type.

use elicitation::Elicit;
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A craps betting action — either a bet/done or a state query.
///
/// Commit variants signal betting intent. When `PlaceBet` is chosen the
/// caller elicits the actual bet amount separately (using the styled `u64`
/// prompt). Explore variants query live table state and loop back for
/// another selection.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Elicit,
    derive_more::Display,
    schemars::JsonSchema,
)]
pub enum CrapsAction {
    /// Place a new bet (amount elicited separately).
    #[display("Place Bet")]
    PlaceBet,
    /// Finished placing bets — ready to roll.
    #[display("Done betting")]
    Done,
    /// Query the current point (if established).
    #[display("View Point")]
    ViewPoint,
    /// Query your active bets on the table.
    #[display("View Active Bets")]
    ViewActiveBets,
    /// Query other players' visible bets.
    #[display("View Other Bets")]
    ViewOtherBets,
    /// Query recent dice roll results.
    #[display("View Roll History")]
    ViewRollHistory,
    /// Query current chip count.
    #[display("View Bankroll")]
    ViewBankroll,
}

impl CrapsAction {
    /// Returns `true` for betting-move variants (PlaceBet, Done).
    #[instrument]
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::PlaceBet | Self::Done)
    }

    /// Returns `true` for state-query variants.
    #[instrument]
    pub fn is_explore(&self) -> bool {
        !self.is_commit()
    }

    /// Maps explore variants to their TypeSpec category name.
    #[instrument]
    pub fn explore_category(&self) -> Option<&'static str> {
        match self {
            Self::ViewPoint => Some("point"),
            Self::ViewActiveBets => Some("active_bets"),
            Self::ViewOtherBets => Some("other_bets"),
            Self::ViewRollHistory => Some("roll_history"),
            Self::ViewBankroll => Some("bankroll"),
            _ => None,
        }
    }
}
