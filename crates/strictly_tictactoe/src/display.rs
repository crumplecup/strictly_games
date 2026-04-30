//! Display mode enum for tic-tac-toe.

use crate::Position;
use elicitation::{Elicit, KaniCompose};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Display strategies for a [`TicTacToeState`](crate::vsm::TicTacToeState).
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
pub enum TttDisplayMode {
    /// Show the current board with the cursor at the given position.
    ///
    /// `cursor` is `None` when no human player is active (e.g. agent-vs-agent).
    #[default]
    Board,
    /// Show the current board with a cursor highlighting a specific cell.
    BoardWithCursor(Position),
    /// Show the full move history.
    BoardHistory,
}
