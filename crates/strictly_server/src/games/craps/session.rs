//! Serializable craps game snapshot for display.

use serde::{Deserialize, Serialize};

/// Snapshot of the current craps game state for TUI display.
///
/// This is a flat, serializable view of the craps session.  It is produced
/// from the live `DisplayPhase<'_>` data inside the craps render loop and
/// passed to [`super::display::GameDisplay`] to generate the AccessKit IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrapsStateView {
    /// Current phase: `"betting"`, `"come_out"`, `"point_phase"`, or `"resolved"`.
    pub phase: String,
    /// Player's current bankroll.
    pub bankroll: u64,
    /// Human-readable summary of table state.
    pub description: String,
    /// Active bets as text lines.
    pub active_bets: Vec<String>,
    /// Current dice roll, if any (e.g. `"4 + 3 = 7"`).
    pub dice_roll: Option<String>,
    /// The established point (e.g. `"6"`), if in point phase.
    pub point: Option<String>,
    /// True when this session phase is terminal.
    pub is_terminal: bool,
}
