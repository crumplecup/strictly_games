//! Typestate graph and event log definitions for the in-game TUI panel.
//!
//! Provides [`NodeDef`], [`EdgeDef`], and [`GameEvent`] data types, plus
//! per-game graph definitions (`*_nodes()`, `*_edges()`, `*_active()`).
//! These are consumed by [`game_ir`] to build the AccessKit IR tree.

use tracing::instrument;

use crate::AnyGame;

// ─────────────────────────────────────────────────────────────
//  Graph definition types
// ─────────────────────────────────────────────────────────────

/// A node in the typestate graph.
#[derive(Debug, Clone)]
pub struct NodeDef {
    /// Display label rendered inside the box.
    pub label: &'static str,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone)]
pub struct EdgeDef {
    /// Index of the source node.
    pub from: usize,
    /// Index of the target node.
    pub to: usize,
    /// Optional short label rendered at the midpoint of the arc.
    ///
    /// Used on skip-forward edges (bypass paths) to name the transition.
    pub label: Option<&'static str>,
}

// ─────────────────────────────────────────────────────────────
//  Game events for the story log
// ─────────────────────────────────────────────────────────────

/// A notable moment in the hand, shown in the story log panel.
#[derive(Debug, Clone)]
pub struct GameEvent {
    /// Display text — should read as plain English.
    pub text: String,
}

impl GameEvent {
    /// A story beat — free-form plain-English narrative.
    pub fn story(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// A phase transition, shown subtly so story beats stand out.
    pub fn phase_change(from: &str, to: &str) -> Self {
        Self {
            text: format!("  {} → {}", from, to),
        }
    }

    /// A proof-carrying contract established (shown dimly — technical detail).
    pub fn proof(proof_name: &str) -> Self {
        Self {
            text: format!("  ✓ {}", proof_name),
        }
    }

    /// Game concluded with a narrative outcome.
    pub fn result(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

// ─────────────────────────────────────────────────────────────
//  Blackjack graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the blackjack typestate graph (in display order).
pub fn blackjack_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "Betting" },
        NodeDef {
            label: "PlayerTurn",
        },
        NodeDef {
            label: "DealerTurn",
        },
        NodeDef { label: "Finished" },
    ]
}

/// Edge definitions for the blackjack typestate graph.
pub fn blackjack_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: None,
        },
        EdgeDef {
            from: 0,
            to: 3,
            label: Some("(natural)"),
        },
    ]
}

/// Maps a blackjack phase name to the active node index.
#[instrument(level = "trace")]
pub fn blackjack_active(phase: &str) -> Option<usize> {
    match phase {
        "Betting" => Some(0),
        "PlayerTurn" => Some(1),
        "DealerTurn" => Some(2),
        "Finished" => Some(3),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────
//  TicTacToe graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the tictactoe typestate graph (in display order).
///
/// The `InProgress` phase is split into `X Turn` and `O Turn` sub-nodes
/// so the graph reflects whose move it is.
pub fn tictactoe_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "GameSetup" },
        NodeDef { label: "X Turn" },
        NodeDef { label: "O Turn" },
        NodeDef {
            label: "GameFinished",
        },
    ]
}

/// Edge definitions for the tictactoe typestate graph.
pub fn tictactoe_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 3,
            label: Some("(end)"),
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: Some("(end)"),
        },
    ]
}

/// Maps the current `AnyGame` to the active node index in the tictactoe graph.
#[instrument(skip(game))]
pub fn tictactoe_active(game: &AnyGame) -> Option<usize> {
    use crate::games::tictactoe::Player;

    match game {
        AnyGame::Setup { .. } => Some(0),
        AnyGame::InProgress { .. } => match game.to_move() {
            Some(Player::X) => Some(1),
            Some(Player::O) => Some(2),
            None => Some(1),
        },
        AnyGame::Won { .. } | AnyGame::Draw { .. } | AnyGame::Finished { .. } => Some(3),
    }
}

/// Returns the phase name string for an `AnyGame` (used for event logging).
pub fn tictactoe_phase_name(game: &AnyGame) -> &'static str {
    use crate::games::tictactoe::Player;

    match game {
        AnyGame::Setup { .. } => "Setup",
        AnyGame::InProgress { .. } => match game.to_move() {
            Some(Player::X) => "X Turn",
            Some(Player::O) => "O Turn",
            None => "InProgress",
        },
        _ => "Finished",
    }
}

// ─────────────────────────────────────────────────────────────
//  Craps graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the craps typestate graph (in display order).
pub fn craps_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "Betting" },
        NodeDef { label: "ComeOut" },
        NodeDef {
            label: "PointPhase",
        },
        NodeDef { label: "Resolved" },
    ]
}

/// Edge definitions for the craps typestate graph.
pub fn craps_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 3,
            label: Some("(natural/craps)"),
        },
        EdgeDef {
            from: 3,
            to: 0,
            label: Some("(next round)"),
        },
    ]
}

/// Maps a craps phase name to the active node index.
#[instrument(level = "trace")]
pub fn craps_active(phase: &str) -> Option<usize> {
    match phase {
        "Betting" => Some(0),
        "ComeOut" => Some(1),
        "PointPhase" => Some(2),
        "Resolved" => Some(3),
        _ => None,
    }
}
