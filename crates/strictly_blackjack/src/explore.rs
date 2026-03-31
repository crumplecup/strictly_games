//! Agent exploration actions for blackjack.
//!
//! [`BlackjackAction`] combines commit actions (Hit, Stand) with explore
//! actions (ViewHand, ViewDealerCard, etc.) in a single [`Select`] enum.
//! The [`Filter`](elicitation::Filter) trait gates which options surface:
//! agents see the full pool, humans see commit-only.

use elicitation::{Elicit, Prompt, Select};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::BasicAction;

/// A blackjack turn action — either a game move or a state query.
///
/// Commit variants execute game moves. Explore variants query live game
/// state and loop back for another selection. Use
/// [`select_with_filter`](elicitation::Select::select_with_filter) to
/// present the appropriate subset per participant type.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Elicit,
    derive_more::Display,
    schemars::JsonSchema,
)]
pub enum BlackjackAction {
    /// Take another card.
    #[display("Hit")]
    Hit,
    /// Keep current hand and end turn.
    #[display("Stand")]
    Stand,
    /// Query cards and total in your hand.
    #[display("View Hand")]
    ViewHand,
    /// Query the dealer's visible up card.
    #[display("View Dealer Card")]
    ViewDealerCard,
    /// Query other players' visible cards.
    #[display("View Other Players")]
    ViewOtherPlayers,
    /// Query how many cards remain in the shoe.
    #[display("View Shoe Status")]
    ViewShoeStatus,
    /// Query current chip count.
    #[display("View Bankroll")]
    ViewBankroll,
}

impl BlackjackAction {
    /// Returns `true` for game-move variants (Hit, Stand).
    #[instrument]
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Hit | Self::Stand)
    }

    /// Returns `true` for state-query variants.
    #[instrument]
    pub fn is_explore(&self) -> bool {
        !self.is_commit()
    }

    /// Extracts the corresponding [`BasicAction`] for commit variants.
    #[instrument]
    pub fn to_basic_action(self) -> Option<BasicAction> {
        match self {
            Self::Hit => Some(BasicAction::Hit),
            Self::Stand => Some(BasicAction::Stand),
            _ => None,
        }
    }

    /// Maps explore variants to their TypeSpec category name.
    #[instrument]
    pub fn explore_category(&self) -> Option<&'static str> {
        match self {
            Self::ViewHand => Some("your_hand"),
            Self::ViewDealerCard => Some("dealer_showing"),
            Self::ViewOtherPlayers => Some("other_players"),
            Self::ViewShoeStatus => Some("shoe_status"),
            Self::ViewBankroll => Some("bankroll"),
            _ => None,
        }
    }
}
