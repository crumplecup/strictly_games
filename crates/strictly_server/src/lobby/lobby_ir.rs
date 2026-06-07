//! Converts lobby screen state to [`VerifiedTree`] for any rendering backend.
//!
//! Each `*_to_verified_tree` function follows the same pipeline as `game_ir.rs`:
//! build AccessKit nodes → wrap in window scaffold → return `VerifiedTree`.
//!
//! # Node layout (all screens)
//!
//! ```text
//! Window (0)          [vertical]
//!   Banner (1)        [title]
//!   Content (2)       [screen-specific]
//!   Status (10_000)   [help / key hints]
//! ```

use std::collections::BTreeMap;

use accesskit::{Node as AkNode, NodeId as AkNodeId, Role as AkRole};
use elicit_accesskit::{NodeId, NodeJson, Role};
use elicit_ui::{VerifiedTree, Viewport};
use tracing::instrument;

use crate::lobby::settings::{GameType, LobbySettings};
use crate::{AggregatedStats, AgentConfig, GameStat, User};

// ── ID constants ──────────────────────────────────────────────────────────────

const WINDOW_ID: u64 = 0;
const BANNER_ID: u64 = 1;
const CONTENT_ID: u64 = 2;
const STATUS_ID: u64 = 10_000;

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Convert `(NodeId, NodeJson)` pairs into the map `VerifiedTree::from_parts` expects.
fn convert_nodes(pairs: Vec<(NodeId, NodeJson)>) -> BTreeMap<AkNodeId, AkNode> {
    pairs
        .into_iter()
        .map(|(id, json)| (id.0, AkNode::from(json)))
        .collect()
}

/// Next available ID after all IDs used in `pairs`.
fn next_id(pairs: &[(NodeId, NodeJson)], fallback: u64) -> u64 {
    pairs
        .iter()
        .map(|(id, _)| id.as_u128() as u64 + 1)
        .max()
        .unwrap_or(fallback)
}

/// Wrap content nodes in the standard Window → Banner → Content → Status scaffold.
fn wrap_in_window(
    title: &str,
    help_text: &str,
    content_id: AkNodeId,
    mut nodes: BTreeMap<AkNodeId, AkNode>,
    viewport: Viewport,
) -> VerifiedTree {
    let mut banner = AkNode::new(AkRole::Banner);
    banner.set_label(title.to_string());
    nodes.insert(AkNodeId::from(BANNER_ID), banner);

    let mut status = AkNode::new(AkRole::Status);
    status.set_label(help_text.to_string());
    nodes.insert(AkNodeId::from(STATUS_ID), status);

    let mut window = AkNode::new(AkRole::Window);
    window.set_children(vec![
        AkNodeId::from(BANNER_ID),
        content_id,
        AkNodeId::from(STATUS_ID),
    ]);
    nodes.insert(AkNodeId::from(WINDOW_ID), window);

    VerifiedTree::from_parts(nodes, AkNodeId::from(WINDOW_ID), viewport)
}

/// Build a `Role::List` subtree.
///
/// `selected_idx` marks one item `is_selected`; `prefix_selected` prepends `▶ `
/// to the active item label (used for all list-based screens).
fn list_nodes(
    items: &[(String, bool)],
    list_label: &str,
    id_base: u64,
) -> (NodeId, Vec<(NodeId, NodeJson)>) {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let root_id = NodeId::from(id_base);
    let mut ctr = id_base + 1;
    let mut child_ids = Vec::with_capacity(items.len());

    for (label, selected) in items {
        let pid = NodeId::from(ctr);
        ctr += 1;
        child_ids.push(pid);
        let display = if *selected {
            format!("▶ {label}")
        } else {
            format!("  {label}")
        };
        pairs.push((pid, NodeJson::new(Role(AkRole::ListItem)).with_label(display)));
    }

    pairs.push((
        root_id,
        NodeJson::new(Role(AkRole::List))
            .with_label(list_label.to_string())
            .with_children(child_ids),
    ));
    (root_id, pairs)
}

// ── Profile select ────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the profile selection screen.
#[instrument(skip(users))]
pub fn profile_select_to_verified_tree(
    users: &[User],
    selected_idx: Option<usize>,
    input_mode: bool,
    new_name_input: &str,
    error_message: Option<&str>,
    viewport: Viewport,
) -> VerifiedTree {
    // User list
    let profile_items: Vec<(String, bool)> = users
        .iter()
        .enumerate()
        .map(|(i, u)| (u.display_name().clone(), selected_idx == Some(i)))
        .collect();
    let (list_root, mut pairs) = list_nodes(&profile_items, "Profiles", 3);

    // Text input for new profile name
    let input_base = next_id(&pairs, 50);
    let input_id = NodeId::from(input_base);
    let input_label = if new_name_input.is_empty() {
        if input_mode {
            "  ".to_string()
        } else {
            "Press 'n' to create a new profile".to_string()
        }
    } else {
        new_name_input.to_string()
    };
    pairs.push((
        input_id,
        NodeJson::new(Role(AkRole::TextInput)).with_label(input_label),
    ));

    // Error status
    let error_base = next_id(&pairs, 60);
    let error_id = NodeId::from(error_base);
    let err_label = error_message.unwrap_or("").to_string();
    pairs.push((
        error_id,
        NodeJson::new(Role(AkRole::Status)).with_label(err_label),
    ));

    // Content group
    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut group = AkNode::new(AkRole::Group);
    group.set_children(vec![list_root.0, input_id.0, error_id.0]);
    nodes.insert(content_id, group);

    let help = if input_mode {
        "Type name | Enter: Confirm | Esc: Cancel"
    } else {
        "↑↓: Select | Enter: Confirm | n: New profile | q: Quit"
    };
    wrap_in_window("Select or Create Profile", help, content_id, nodes, viewport)
}

// ── Main lobby ────────────────────────────────────────────────────────────────

/// Menu option labels for the main lobby.
pub fn lobby_menu_items(selected_game: GameType, selected_idx: usize) -> Vec<(String, bool)> {
    let labels = [
        "Play Game".to_string(),
        format!("Select Game  [ {} ]", selected_game.label()),
        "View Statistics".to_string(),
        "Change Profile".to_string(),
        "Settings".to_string(),
        "Quit".to_string(),
    ];
    labels
        .into_iter()
        .enumerate()
        .map(|(i, l)| (l, i == selected_idx))
        .collect()
}

/// Build the [`VerifiedTree`] for the main lobby screen.
#[instrument(skip(user, stats))]
pub fn main_lobby_to_verified_tree(
    user: &User,
    stats: Option<&AggregatedStats>,
    selected_game: GameType,
    selected_idx: usize,
    viewport: Viewport,
) -> VerifiedTree {
    // Stats bar
    let stats_text = match stats {
        Some(s) => format!(
            "Player: {}   W:{} / L:{} / D:{}   Win rate: {:.1}%",
            user.display_name(),
            s.wins(),
            s.losses(),
            s.draws(),
            s.win_rate()
        ),
        None => format!("Player: {}", user.display_name()),
    };
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let stats_id = NodeId::from(3u64);
    pairs.push((
        stats_id,
        NodeJson::new(Role(AkRole::Status)).with_label(stats_text),
    ));

    // Menu list
    let menu_items = lobby_menu_items(selected_game, selected_idx);
    let (menu_root, menu_pairs) = list_nodes(&menu_items, "Menu", 4);
    pairs.extend(menu_pairs);

    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut group = AkNode::new(AkRole::Group);
    group.set_children(vec![stats_id.0, menu_root.0]);
    nodes.insert(content_id, group);

    wrap_in_window(
        "Strictly Games — Lobby",
        "↑↓: Navigate | Enter: Select | q: Quit",
        content_id,
        nodes,
        viewport,
    )
}

// ── Game select ───────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the game selection screen.
#[instrument]
pub fn game_select_to_verified_tree(selected_idx: usize, viewport: Viewport) -> VerifiedTree {
    let items: Vec<(String, bool)> = GameType::all()
        .iter()
        .enumerate()
        .map(|(i, g)| (g.label().to_string(), i == selected_idx))
        .collect();
    let (list_root, pairs) = list_nodes(&items, "Games", 3);

    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![list_root.0]);
    nodes.insert(content_id, content);

    wrap_in_window(
        "Select Game",
        "↑↓: Navigate | Enter: Select | Esc: Back",
        content_id,
        nodes,
        viewport,
    )
}

// ── Agent select ──────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the agent selection screen.
#[instrument(skip(agents))]
pub fn agent_select_to_verified_tree(
    agents: &[AgentConfig],
    selected_idx: Option<usize>,
    viewport: Viewport,
) -> VerifiedTree {
    let items: Vec<(String, bool)> = if agents.is_empty() {
        vec![(
            "No agents found — check your config directory".to_string(),
            false,
        )]
    } else {
        agents
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let label = format!("{} ({:?} / {})", a.name(), a.llm_provider(), a.llm_model());
                (label, selected_idx == Some(i))
            })
            .collect()
    };

    let (list_root, pairs) = list_nodes(&items, "Agents", 3);
    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![list_root.0]);
    nodes.insert(content_id, content);

    wrap_in_window(
        "Select AI Opponent",
        "↑↓: Select | Enter: Start Game | Esc: Back | q: Quit",
        content_id,
        nodes,
        viewport,
    )
}

// ── Blackjack setup ───────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the blackjack table setup screen.
#[instrument(skip(agents, seated))]
pub fn blackjack_setup_to_verified_tree(
    human_name: &str,
    agents: &[AgentConfig],
    seated: &[bool],
    selected_idx: Option<usize>,
    viewport: Viewport,
) -> VerifiedTree {
    let seated_count = seated.iter().filter(|&&s| s).count();

    // Human seat paragraph
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let human_id = NodeId::from(3u64);
    pairs.push((
        human_id,
        NodeJson::new(Role(AkRole::ListItem)).with_label(format!(
            "✔  {} (You)  —  bankroll: 1000 chips",
            human_name
        )),
    ));

    // Agent checklist
    let agent_items: Vec<(String, bool)> = if agents.is_empty() {
        vec![("No agents in library".to_string(), false)]
    } else {
        agents
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let check = if seated[i] { "✔" } else { "○" };
                let cursor = if selected_idx == Some(i) { "▶ " } else { "  " };
                (format!("{cursor}{check}  {}", a.name()), false)
            })
            .collect()
    };

    let list_label = format!("Add AI Agents ({seated_count}/3 selected)");
    let (agent_root, agent_pairs) = list_nodes(&agent_items, &list_label, 10);
    pairs.extend(agent_pairs);

    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![human_id.0, agent_root.0]);
    nodes.insert(content_id, content);

    wrap_in_window(
        "♠  Blackjack Table Setup  ♠",
        "↑↓: Navigate  Space/Enter: Toggle agent  s: Start game  Esc: Back",
        content_id,
        nodes,
        viewport,
    )
}

// ── Settings ──────────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the settings screen.
#[instrument]
pub fn settings_to_verified_tree(
    settings: &LobbySettings,
    selected_idx: usize,
    viewport: Viewport,
) -> VerifiedTree {
    let checkbox = |b: bool| if b { "[✓]" } else { "[ ]" };
    let items = vec![
        (
            format!(
                "Who Goes First?    [ {} ]",
                settings.first_player.label()
            ),
            selected_idx == 0,
        ),
        (
            format!(
                "Show Typestate Graph   {}",
                checkbox(settings.show_typestate_graph)
            ),
            selected_idx == 1,
        ),
    ];

    let (list_root, pairs) = list_nodes(&items, "Preferences", 3);
    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![list_root.0]);
    nodes.insert(content_id, content);

    wrap_in_window(
        "Settings",
        "↑↓: Navigate | Enter / Space: Toggle | Esc: Back",
        content_id,
        nodes,
        viewport,
    )
}

// ── Stats view ────────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the statistics view screen.
#[instrument(skip(user, aggregated, recent_games))]
pub fn stats_view_to_verified_tree(
    user: &User,
    aggregated: Option<&AggregatedStats>,
    recent_games: &[GameStat],
    viewport: Viewport,
) -> VerifiedTree {
    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();

    // Summary paragraph
    let summary_text = match aggregated {
        Some(s) => format!(
            "Games: {}   Wins: {}   Losses: {}   Draws: {}   Win Rate: {:.1}%",
            s.total_games(),
            s.wins(),
            s.losses(),
            s.draws(),
            s.win_rate()
        ),
        None => "No statistics available".to_string(),
    };
    let summary_id = NodeId::from(3u64);
    pairs.push((
        summary_id,
        NodeJson::new(Role(AkRole::Status))
            .with_label(summary_text)
            .with_description("Summary".to_string()),
    ));

    // Recent games as list items
    let mut game_ids: Vec<NodeId> = Vec::new();
    let mut ctr = 4u64;
    for stat in recent_games.iter().take(20) {
        let pid = NodeId::from(ctr);
        ctr += 1;
        game_ids.push(pid);
        let label = format!(
            "{}  |  {}  |  {}  |  {} moves",
            stat.opponent_name(),
            stat.game_type(),
            stat.outcome(),
            stat.moves_count()
        );
        pairs.push((pid, NodeJson::new(Role(AkRole::ListItem)).with_label(label)));
    }
    let games_root = NodeId::from(ctr);
    pairs.push((
        games_root,
        NodeJson::new(Role(AkRole::List))
            .with_label("Recent Games (20 most recent)".to_string())
            .with_children(game_ids),
    ));

    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![summary_id.0, games_root.0]);
    nodes.insert(content_id, content);

    let title = format!("Statistics — {}", user.display_name());
    wrap_in_window(
        &title,
        "Esc / b: Back to Lobby | q: Quit",
        content_id,
        nodes,
        viewport,
    )
}

// ── In-game ───────────────────────────────────────────────────────────────────

/// Build the [`VerifiedTree`] for the in-game placeholder screen.
#[instrument]
pub fn in_game_to_verified_tree(
    agent_name: &str,
    game_finished: bool,
    result_message: Option<&str>,
    viewport: Viewport,
) -> VerifiedTree {
    let text = if game_finished {
        result_message
            .unwrap_or("Game finished.")
            .to_string()
            + "\n\nPress any key to return to lobby."
    } else {
        format!(
            "Game in progress vs {agent_name}…\n\nThe game loop is running.\nPress any key to return to lobby."
        )
    };

    let mut pairs: Vec<(NodeId, NodeJson)> = Vec::new();
    let msg_id = NodeId::from(3u64);
    pairs.push((
        msg_id,
        NodeJson::new(Role(AkRole::Status)).with_label(text),
    ));

    let mut nodes = convert_nodes(pairs);
    let content_id = AkNodeId::from(CONTENT_ID);
    let mut content = AkNode::new(AkRole::Group);
    content.set_children(vec![msg_id.0]);
    nodes.insert(content_id, content);

    wrap_in_window("In Game", "q: Quit | any key: Return to Lobby", content_id, nodes, viewport)
}
