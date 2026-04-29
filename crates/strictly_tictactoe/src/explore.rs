//! Agent exploration actions for tic-tac-toe.
//!
//! [`TicTacToeAction`] combines the commit action Play(Position) with
//! explore actions (ViewBoard, ViewLegalMoves, ViewThreats) in a single
//! [`Select`] enum. The [`Filter`](elicitation::Filter) trait gates which
//! options surface per participant type.

use elicitation::Elicit;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::Position;

/// A tic-tac-toe turn action — either a move or a state query.
///
/// The `Play` variant commits a move at the given position. Explore
/// variants query live game state and loop back for another selection.
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
pub enum TicTacToeAction {
    /// Place your mark at a board position.
    #[display("Play {_0}")]
    Play(Position),
    /// Query the current board layout.
    #[display("View Board")]
    ViewBoard,
    /// Query which positions are still open.
    #[display("View Legal Moves")]
    ViewLegalMoves,
    /// Query immediate win/block opportunities.
    #[display("View Threats")]
    ViewThreats,
}

impl TicTacToeAction {
    /// Returns `true` for game-move variants.
    #[instrument]
    pub fn is_commit(&self) -> bool {
        matches!(self, Self::Play(_))
    }

    /// Returns `true` for state-query variants.
    #[instrument]
    pub fn is_explore(&self) -> bool {
        !self.is_commit()
    }

    /// Extracts the [`Position`] from a commit variant.
    #[instrument]
    pub fn to_position(self) -> Option<Position> {
        match self {
            Self::Play(pos) => Some(pos),
            _ => None,
        }
    }

    /// Maps explore variants to their TypeSpec category name.
    #[instrument]
    pub fn explore_category(&self) -> Option<&'static str> {
        match self {
            Self::ViewBoard => Some("board"),
            Self::ViewLegalMoves => Some("legal_moves"),
            Self::ViewThreats => Some("threats"),
            _ => None,
        }
    }
}
