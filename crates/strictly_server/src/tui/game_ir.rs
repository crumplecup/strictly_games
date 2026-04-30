//! Converts game display types to [`VerifiedTree`] for any rendering backend.
//!
//! Each `*_to_verified_tree` function follows the same three-step pipeline as
//! `nav_tree_to_verified_tree` in the archive frontend:
//!
//! 1. `GameDisplay::to_ak_nodes` → AccessKit IR sub-tree
//! 2. Wrap in window/banner/status scaffolding
//! 3. `VerifiedTree::from_parts` → structural WCAG credential anchor
//!
//! # TTT node layout
//!
//! ```text
//! Window (0)     [vertical]
//!   Banner (1)   [horizontal — title text]
//!   Row (2)      [horizontal — equal-fill columns]
//!     Main (4+)  — board + status (GameDisplay::to_ak_nodes)
//!     List       — event log (story column)
//!     List       — dialogue chat (optional)
//!     Tree       — typestate graph (optional)
//!   Status (10_000) — status text
//! ```
//!
//! BJ and Craps use the simpler single-column layout (no chat or typestate).

use std::collections::BTreeMap;

use accesskit::{Node as AkNode, NodeId as AkNodeId, Role as AkRole};
use elicit_accesskit::{NodeId, NodeJson, Role};
use elicit_ui::{VerifiedTree, Viewport};
use tracing::instrument;

use crate::games::blackjack::BlackjackStateView;
use crate::games::craps::CrapsStateView;
use crate::games::display::GameDisplay;
use crate::games::tictactoe::AnyGame;
use crate::session::DialogueEntry;
use crate::tui::typestate_widget::{EdgeDef, GameEvent, NodeDef};
use strictly_blackjack::BlackjackDisplayMode;
use strictly_craps::CrapsDisplayMode;
use strictly_tictactoe::TttDisplayMode;

// ── Public parameter types ────────────────────────────────────────────────────

/// Groups the three typestate-graph parameters passed to `*_to_verified_tree`.
///
/// When `nodes` is empty the graph column is omitted entirely.
pub struct GraphParams<'a> {
    /// State-node definitions produced by the VSM graph walk.
    pub nodes: &'a [NodeDef],
    /// Transition-edge definitions produced by the VSM graph walk.
    pub edges: &'a [EdgeDef],
    /// Index of the currently active node (highlighted in the graph).
    pub active: Option<usize>,
}

/// Groups the event-log and dialogue parameters passed to `*_to_verified_tree`.
pub struct EventLog<'a> {
    /// Ordered game story entries (actions, outcomes).
    pub events: &'a [GameEvent],
    /// Chat dialogue between participants (empty for single-player).
    pub dialogue: &'a [DialogueEntry],
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Convert `(NodeId, NodeJson)` pairs from `to_ak_nodes` into the
/// `BTreeMap<accesskit::NodeId, accesskit::Node>` that [`VerifiedTree::from_parts`] expects.
fn convert_nodes(pairs: Vec<(NodeId, NodeJson)>) -> BTreeMap<accesskit::NodeId, accesskit::Node> {
    pairs
        .into_iter()
        .map(|(eid, json)| (eid.0, accesskit::Node::from(json)))
        .collect()
}

/// Returns the next available ID after all IDs used in `pairs`.
fn next_after(pairs: &[(NodeId, NodeJson)], fallback: u64) -> u64 {
    pairs
        .iter()
        .map(|(id, _)| id.as_u128() as u64 + 1)
        .max()
        .unwrap_or(fallback)
}

/// Build an event-log subtree.
///
/// Returns `(root_id, pairs)` where root is `Role::List` and each item is
/// `Role::ListItem`.  `bridge_list` renders this as a ratatui `List` widget.
fn event_log_nodes(events: &[GameEvent], id_base: u64) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);

    let mut item_ids: Vec<NodeId> = Vec::with_capacity(events.len());
    for (ctr, event) in (id_base + 1..).zip(events.iter()) {
        let pid = NodeId::from(ctr);
        item_ids.push(pid);
        pairs.push((
            pid,
            NodeJson::new(Role(AkRole::ListItem)).with_label(event.text.clone()),
        ));
    }
    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label("Game Story".to_string())
            .with_children(item_ids),
    ));
    (root_id, pairs)
}

/// Build a dialogue (chat) subtree.
///
/// Returns `(root_id, pairs)` where root is `Role::List` and each item is
/// `Role::ListItem` labelled `"Role: text"`.  `bridge_list` renders as a
/// ratatui `List` widget.
fn chat_nodes(dialogue: &[DialogueEntry], id_base: u64) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);

    let mut item_ids: Vec<NodeId> = Vec::with_capacity(dialogue.len());
    for (ctr, entry) in (id_base + 1..).zip(dialogue.iter()) {
        let pid = NodeId::from(ctr);
        item_ids.push(pid);
        pairs.push((
            pid,
            NodeJson::new(Role(AkRole::ListItem))
                .with_label(format!("{}: {}", entry.role, entry.text)),
        ));
    }
    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label("Chat".to_string())
            .with_children(item_ids),
    ));
    (root_id, pairs)
}

/// Build a typestate-graph subtree.
///
/// Returns `(root_id, pairs)` where root is `Role::List` with each state as a
/// `Role::ListItem` and each edge transition as a description child item.
/// The active state is prefixed with `▶` in its label.
/// Edges are rendered as `"From → To"` (with optional label) for screen reader navigation.
fn typestate_nodes(
    graph_nodes: &[NodeDef],
    graph_edges: &[EdgeDef],
    active: Option<usize>,
    id_base: u64,
) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);
    let mut ctr = id_base + 1;

    let mut item_ids: Vec<NodeId> = Vec::with_capacity(graph_nodes.len() + graph_edges.len());
    for (i, node_def) in graph_nodes.iter().enumerate() {
        let pid = NodeId::from(ctr);
        ctr += 1;
        item_ids.push(pid);
        let label = if active == Some(i) {
            format!("▶ {}", node_def.label)
        } else {
            format!("  {}", node_def.label)
        };
        pairs.push((pid, NodeJson::new(Role(AkRole::ListItem)).with_label(label)));
    }
    // Add transition edges so screen readers can navigate the state machine.
    for edge in graph_edges {
        let from_label = graph_nodes.get(edge.from).map(|n| n.label).unwrap_or("?");
        let to_label = graph_nodes.get(edge.to).map(|n| n.label).unwrap_or("?");
        let edge_label = if let Some(lbl) = edge.label {
            format!("  {} → {} {}", from_label, to_label, lbl)
        } else {
            format!("  {} → {}", from_label, to_label)
        };
        let pid = NodeId::from(ctr);
        ctr += 1;
        item_ids.push(pid);
        pairs.push((
            pid,
            NodeJson::new(Role(AkRole::ListItem))
                .with_label(edge_label)
                .with_description("transition".to_string()),
        ));
    }
    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label("Game States".to_string())
            .with_children(item_ids),
    ));
    (root_id, pairs)
}

/// Build an agent-panel subtree.
///
/// Each agent is a `Role::ListItem` labelled `"Name — phase: description"`.
/// Returns `(root_id, pairs)` with a `Role::List` root.
fn agent_nodes(agents: &[(&str, &str, &str)], id_base: u64) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);

    let mut item_ids: Vec<NodeId> = Vec::with_capacity(agents.len());
    for (ctr, (name, phase, description)) in (id_base + 1..).zip(agents.iter().copied()) {
        let pid = NodeId::from(ctr);
        item_ids.push(pid);
        pairs.push((
            pid,
            NodeJson::new(Role(AkRole::ListItem))
                .with_label(format!("{name} [{phase}]: {description}")),
        ));
    }
    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label("Agents".to_string())
            .with_children(item_ids),
    ));
    (root_id, pairs)
}

/// Build an available-tools subtree.
///
/// Each tool description becomes a `Role::ListItem`.
/// Returns `(root_id, pairs)` with a `Role::List` root.
fn tools_nodes(tools: &[String], id_base: u64) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);

    let mut item_ids: Vec<NodeId> = Vec::with_capacity(tools.len());
    for (i, tool) in tools.iter().enumerate() {
        let pid = NodeId::from(id_base + 1 + i as u64);
        item_ids.push(pid);
        let key = if i < 9 {
            format!("[{}]", i + 1)
        } else {
            format!("[{}]", (b'a' + (i as u8 - 9)) as char)
        };
        pairs.push((
            pid,
            NodeJson::new(Role(AkRole::ListItem)).with_label(format!("{key} {tool}")),
        ));
    }
    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label("Controls".to_string())
            .with_children(item_ids),
    ));
    (root_id, pairs)
}
///
/// - `NodeId(0)` — `Role::Window` (vertical layout)
/// - `NodeId(1)` — `Role::Banner` (horizontal — title bar)
/// - `row_id` — `Role::Row` (horizontal — content columns)
/// - `NodeId(10_000)` — `Role::Status` (status text)
fn wrap_in_window_with_row(
    title: &str,
    status_text: &str,
    row_id: AkNodeId,
    mut nodes: BTreeMap<AkNodeId, AkNode>,
    viewport: Viewport,
) -> VerifiedTree {
    let banner_id = AkNodeId::from(1u64);
    let mut banner = AkNode::new(AkRole::Banner);
    banner.set_label(title.to_string());
    nodes.insert(banner_id, banner);

    let status_id = AkNodeId::from(10_000u64);
    let mut status = AkNode::new(AkRole::Status);
    status.set_label(status_text.to_string());
    nodes.insert(status_id, status);

    let window_id = AkNodeId::from(0u64);
    let mut window = AkNode::new(AkRole::Window);
    window.set_children(vec![banner_id, row_id, status_id]);
    nodes.insert(window_id, window);

    VerifiedTree::from_parts(nodes, window_id, viewport)
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Build the full multi-column [`VerifiedTree`] for the TTT TUI frame.
///
/// Produces a complete WCAG-verified AccessKit tree that covers all UI
/// regions: board, event log, chat, and typestate graph.  The `Role::Row`
/// container maps to `horizontal_layout` in the ratatui bridge so all
/// columns receive equal-fill width.
///
/// The returned tree is the `WcagVerified` credential anchor for
/// `TttUiConsistent`.
#[instrument(skip(game, log, graph))]
pub fn ttt_to_verified_tree(
    game: &AnyGame,
    mode: &TttDisplayMode,
    log: &EventLog<'_>,
    graph: &GraphParams<'_>,
    viewport: Viewport,
) -> VerifiedTree {
    // id=0: Window, id=1: Banner, id=2: Row, id=3+: board; 10_000: status
    let (board_root, board_pairs) = game.to_ak_nodes(mode, 3);
    let mut all_pairs = board_pairs;

    let story_base = next_after(&all_pairs, 4);
    let (story_root, story_pairs) = event_log_nodes(log.events, story_base);
    all_pairs.extend(story_pairs);

    let mut col_roots = vec![board_root.0, story_root.0];

    if !log.dialogue.is_empty() {
        let chat_base = next_after(&all_pairs, story_base + 1);
        let (chat_root, chat_pairs) = chat_nodes(log.dialogue, chat_base);
        all_pairs.extend(chat_pairs);
        col_roots.push(chat_root.0);
    }

    if !graph.nodes.is_empty() {
        let ts_base = next_after(&all_pairs, 100);
        let (ts_root, ts_pairs) = typestate_nodes(graph.nodes, graph.edges, graph.active, ts_base);
        all_pairs.extend(ts_pairs);
        col_roots.push(ts_root.0);
    }

    let mut nodes = convert_nodes(all_pairs);

    // Row (id=2) — horizontal content container
    let row_id = AkNodeId::from(2u64);
    let mut row = AkNode::new(AkRole::Row);
    row.set_children(col_roots);
    nodes.insert(row_id, row);

    let status = if game.is_over() {
        "Game over".to_string()
    } else {
        "Game in progress".to_string()
    };
    wrap_in_window_with_row("Tic-Tac-Toe", &status, row_id, nodes, viewport)
}

/// Build the full multi-column [`VerifiedTree`] for the Blackjack TUI frame.
///
/// Columns (each becomes an equal-fill column via `Role::Row`):
/// - Human game state (`view`, main game subtree)
/// - Agent panels (one list item per agent)
/// - Event log
/// - Available tools / controls
/// - Dialogue chat (if non-empty)
/// - Typestate graph (if `graph_nodes` non-empty)
#[instrument(skip(view, agents, log, tools, graph))]
pub fn bj_to_verified_tree(
    view: &BlackjackStateView,
    mode: &BlackjackDisplayMode,
    agents: &[(&str, &str, &str)],
    log: &EventLog<'_>,
    tools: &[String],
    graph: &GraphParams<'_>,
    viewport: Viewport,
) -> VerifiedTree {
    // id=0: Window, id=1: Banner, id=2: Row, id=3+: human subtree; 10_000: status
    let (human_root, human_pairs) = view.to_ak_nodes(mode, 3);
    let mut all_pairs = human_pairs;

    let mut col_roots = vec![human_root.0];

    if !agents.is_empty() {
        let agents_base = next_after(&all_pairs, 4);
        let (agents_root, agents_pairs) = agent_nodes(agents, agents_base);
        all_pairs.extend(agents_pairs);
        col_roots.push(agents_root.0);
    }

    let events_base = next_after(&all_pairs, 100);
    let (events_root, events_pairs) = event_log_nodes(log.events, events_base);
    all_pairs.extend(events_pairs);
    col_roots.push(events_root.0);

    if !tools.is_empty() {
        let tools_base = next_after(&all_pairs, events_base + 1);
        let (tools_root, tools_pairs) = tools_nodes(tools, tools_base);
        all_pairs.extend(tools_pairs);
        col_roots.push(tools_root.0);
    }

    if !log.dialogue.is_empty() {
        let chat_base = next_after(&all_pairs, 200);
        let (chat_root, chat_pairs) = chat_nodes(log.dialogue, chat_base);
        all_pairs.extend(chat_pairs);
        col_roots.push(chat_root.0);
    }

    if !graph.nodes.is_empty() {
        let ts_base = next_after(&all_pairs, 300);
        let (ts_root, ts_pairs) = typestate_nodes(graph.nodes, graph.edges, graph.active, ts_base);
        all_pairs.extend(ts_pairs);
        col_roots.push(ts_root.0);
    }

    let mut nodes = convert_nodes(all_pairs);

    let row_id = AkNodeId::from(2u64);
    let mut row = AkNode::new(AkRole::Row);
    row.set_children(col_roots);
    nodes.insert(row_id, row);

    let status = format!("Blackjack — {} | Bankroll: ${}", view.phase, view.bankroll);
    wrap_in_window_with_row("Blackjack", &status, row_id, nodes, viewport)
}

/// Build the full multi-column [`VerifiedTree`] for the Craps TUI frame.
///
/// Columns (each becomes an equal-fill column via `Role::Row`):
/// - Game state (view, main game subtree)
/// - Event log
/// - Dialogue chat (if non-empty)
/// - Typestate graph (if `graph_nodes` non-empty)
#[instrument(skip(view, log, graph))]
pub fn craps_to_verified_tree(
    view: &CrapsStateView,
    mode: &CrapsDisplayMode,
    log: &EventLog<'_>,
    graph: &GraphParams<'_>,
    viewport: Viewport,
) -> VerifiedTree {
    // id=0: Window, id=1: Banner, id=2: Row, id=3+: game subtree; 10_000: status
    let (game_root, game_pairs) = view.to_ak_nodes(mode, 3);
    let mut all_pairs = game_pairs;

    let events_base = next_after(&all_pairs, 100);
    let (events_root, events_pairs) = event_log_nodes(log.events, events_base);
    all_pairs.extend(events_pairs);

    let mut col_roots = vec![game_root.0, events_root.0];

    if !log.dialogue.is_empty() {
        let chat_base = next_after(&all_pairs, 200);
        let (chat_root, chat_pairs) = chat_nodes(log.dialogue, chat_base);
        all_pairs.extend(chat_pairs);
        col_roots.push(chat_root.0);
    }

    if !graph.nodes.is_empty() {
        let ts_base = next_after(&all_pairs, 300);
        let (ts_root, ts_pairs) = typestate_nodes(graph.nodes, graph.edges, graph.active, ts_base);
        all_pairs.extend(ts_pairs);
        col_roots.push(ts_root.0);
    }

    let mut nodes = convert_nodes(all_pairs);

    let row_id = AkNodeId::from(2u64);
    let mut row = AkNode::new(AkRole::Row);
    row.set_children(col_roots);
    nodes.insert(row_id, row);

    let status = format!("Craps — {} | Bankroll: ${}", view.phase, view.bankroll);
    wrap_in_window_with_row("Craps", &status, row_id, nodes, viewport)
}
