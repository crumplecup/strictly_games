//! Frontend-agnostic game session state.
//!
//! Each session task polls the HTTP game server and writes its results into an
//! `Arc<RwLock<*ViewState>>`.  Any frontend reads the view state each frame,
//! converts it to a [`elicit_ui::VerifiedTree`] via `*_to_verified_tree`, and
//! renders through its own IR bridge — ratatui, egui, leptos, etc.
//!
//! # Session lifecycle
//!
//! ```text
//! start_blackjack_session() / start_ttt_session()
//!   └─ spawn HTTP server + agent subprocess
//!   └─ connect REST client
//!   └─ tokio::spawn(poll_loop)          // updates Arc<RwLock<*ViewState>>
//!   └─ return *SessionHandle
//!
//! Frontend each frame:
//!   state = handle.state.read()
//!   tree  = *_to_verified_tree(&state, ...)
//!   (widget/node, _, _) = Backend::render(&tree)
//!   // render widget
//! ```

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Verify `port` is free; if not, ask the OS for an unused port.
///
/// When the user specifies a port via CLI it is used as-is.  Only if that
/// port is already bound do we fall back to an OS-allocated free port.
fn ensure_free_port(port: u16) -> u16 {
    if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
        port
    } else {
        tracing::warn!(
            requested_port = port,
            "port already in use, selecting a free port"
        );
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .ok()
            .and_then(|l| l.local_addr().ok())
            .map(|a| a.port())
            .unwrap_or(port)
    }
}

use tracing::instrument;

use crate::AnyGame;
use crate::FirstPlayer;
use crate::Position;
use crate::TicTacToePlayer;
use crate::games::blackjack::BlackjackStateView;
use crate::session::DialogueEntry;
use crate::tui::blackjack::BlackjackSessionOutcome;
use crate::tui::rest_client::BlackjackTool;
use crate::tui::typestate_widget::GameEvent;

// ── Blackjack session ─────────────────────────────────────────────────────────

/// All state needed to render one blackjack frame — updated by the session task,
/// read by any frontend.
#[derive(Debug, Clone)]
pub struct BlackjackViewState {
    /// Human player's seat state, converted to the display IR type.
    pub bj_view: BlackjackStateView,
    /// Agent seats: (name, phase, description) triples for the agent panel.
    pub agent_triples: Vec<(String, String, String)>,
    /// Ordered story entries for the Game Story panel.
    pub event_log: Vec<GameEvent>,
    /// Merged agent dialogue for the Chat panel.
    pub merged_dialogue: Vec<DialogueEntry>,
    /// Human-readable descriptions of currently callable tools (for Controls panel).
    pub tool_descs: Vec<String>,
    /// Full tool list — used by frontends to map key presses to tool calls.
    pub available_tools: Vec<BlackjackTool>,
    /// Index of the active typestate node (for the typestate graph column).
    pub active_node: Option<usize>,
    /// Set when the session ends naturally (win/loss/push); `None` while running.
    pub outcome: Option<BlackjackSessionOutcome>,
}

impl Default for BlackjackViewState {
    fn default() -> Self {
        Self {
            bj_view: BlackjackStateView {
                phase: "connecting".to_string(),
                bankroll: 0,
                description: "Connecting to game server…".to_string(),
                is_terminal: false,
                player_hands: vec![],
                dealer_hand: vec![],
            },
            agent_triples: vec![],
            event_log: vec![],
            merged_dialogue: vec![],
            tool_descs: vec![],
            available_tools: vec![],
            active_node: None,
            outcome: None,
        }
    }
}

/// Actions a frontend can send to the blackjack session task.
#[derive(Debug)]
pub enum BlackjackAction {
    /// Call an MCP tool on the human seat (e.g. `blackjack__hit`, `blackjack__place`).
    CallTool {
        /// Fully-qualified tool name.
        name: String,
        /// Tool arguments (JSON object).
        args: serde_json::Value,
    },
    /// Player quit the session.
    Quit,
}

/// Handle to a running blackjack session.
///
/// The session task updates [`state`] continuously; any frontend reads it each
/// frame.  Player actions are sent via [`action_tx`].
pub struct BlackjackSessionHandle {
    /// Shared session state — read-lock each frame to render.
    pub state: Arc<RwLock<BlackjackViewState>>,
    /// Send player actions to the session task.
    pub action_tx: tokio::sync::mpsc::Sender<BlackjackAction>,
}

/// Start a blackjack session and return a handle the frontend can render from.
///
/// Returns immediately — the handle's shared state starts in "connecting" mode
/// while a background tokio task spawns the HTTP server, connects the client,
/// spawns agent subprocesses, and then polls game state continuously.
///
/// Call this from a synchronous context (e.g. a winit event callback running
/// inside a tokio runtime) — it uses `tokio::spawn` internally, never `block_on`.
#[instrument(skip_all, fields(port, num_players = players.len()))]
pub fn start_blackjack_session(
    players: Vec<crate::PlayerSlot>,
    port: u16,
    fallback_agent_config: PathBuf,
    show_typestate_graph: bool,
) -> BlackjackSessionHandle {
    // Create the handle immediately — synchronous, no await.
    // The shared state starts in "connecting" mode while the background task
    // does the async server/client setup.
    let state = Arc::new(RwLock::new(BlackjackViewState::default()));
    let (action_tx, action_rx) = tokio::sync::mpsc::channel(32);

    let state_task = state.clone();
    tokio::spawn(blackjack_setup_task(
        state_task,
        action_rx,
        players,
        port,
        fallback_agent_config,
        show_typestate_graph,
    ));

    BlackjackSessionHandle { state, action_tx }
}

/// Background task: async server/agent setup followed by the poll loop.
///
/// Runs as a tokio task so the frontend never blocks waiting for setup.
#[instrument(skip_all)]
async fn blackjack_setup_task(
    state: Arc<RwLock<BlackjackViewState>>,
    action_rx: tokio::sync::mpsc::Receiver<BlackjackAction>,
    players: Vec<crate::PlayerSlot>,
    port: u16,
    fallback_agent_config: PathBuf,
    show_typestate_graph: bool,
) {
    use crate::PlayerKind;
    use crate::PlayerSlot;
    use crate::tui::rest_client::{BlackjackObserver, HumanBlackjackClient};
    use crate::tui::standalone::{GameMode, ProcessGuards, spawn_agent, spawn_server};

    const HUMAN_SESSION: &str = "human_bj";

    // Use the requested port if free; fall back to an OS-allocated port only
    // if the requested port is already bound.
    let port = ensure_free_port(port);
    tracing::debug!(port, "blackjack session port selected");

    let human_slot = players
        .iter()
        .find(|s| matches!(s.kind, PlayerKind::Human))
        .cloned()
        .unwrap_or_else(|| PlayerSlot {
            name: "You".to_string(),
            bankroll: 1_000,
            kind: PlayerKind::Human,
        });

    let agent_slots: Vec<PlayerSlot> = players
        .into_iter()
        .filter(|s| matches!(s.kind, PlayerKind::Agent(_)))
        .collect();

    let player_name = human_slot.name.clone();
    let initial_bankroll = human_slot.bankroll;
    let server_url = format!("http://localhost:{port}");

    let server = match spawn_server(port).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "Failed to spawn game server");
            if let Ok(mut s) = state.write() {
                s.bj_view.description = format!("Error: {e}");
            }
            return;
        }
    };

    let human = match HumanBlackjackClient::connect(server_url.clone()).await {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to game server");
            if let Ok(mut s) = state.write() {
                s.bj_view.description = format!("Error: {e}");
            }
            return;
        }
    };

    // Initialise the shared table — must be called before spawning agents.
    let num_seats = (1 + agent_slots.len()) as u64;
    if let Err(e) = human
        .call_tool(
            "blackjack_deal",
            serde_json::json!({
                "initial_bankroll": initial_bankroll,
                "session_id": HUMAN_SESSION,
                "num_seats": num_seats,
                "player_name": player_name
            }),
        )
        .await
    {
        tracing::error!(error = %e, "blackjack_deal init failed");
        if let Ok(mut s) = state.write() {
            s.bj_view.description = format!("Error starting game: {e}");
        }
        return;
    }

    let mut agent_children: Vec<tokio::process::Child> = Vec::new();
    let mut agent_session_ids: Vec<String> = Vec::new();
    for (i, slot) in agent_slots.iter().enumerate() {
        let sid = format!("agent_bj_{i}");
        let config_path = match &slot.kind {
            PlayerKind::Agent(cfg) => cfg
                .config_path()
                .clone()
                .unwrap_or_else(|| fallback_agent_config.clone()),
            PlayerKind::Human => fallback_agent_config.clone(),
        };
        match spawn_agent(
            port,
            config_path,
            GameMode::Blackjack {
                bankroll: slot.bankroll,
                session_id: sid.clone(),
            },
        )
        .await
        {
            Ok(child) => {
                agent_children.push(child);
                agent_session_ids.push(sid);
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to spawn agent");
            }
        }
    }
    let guards = ProcessGuards::many(server, agent_children);

    let human_observer = BlackjackObserver::new(server_url.clone(), HUMAN_SESSION.to_string());
    let agent_observers: Vec<BlackjackObserver> = agent_session_ids
        .iter()
        .map(|sid| BlackjackObserver::new(server_url.clone(), sid.clone()))
        .collect();

    if let Ok(mut s) = state.write() {
        s.event_log = vec![GameEvent::story(format!(
            "🃏  Blackjack — {player_name} joined (bankroll: ${initial_bankroll})"
        ))];
    }

    blackjack_poll_task(
        state,
        action_rx,
        human,
        human_observer,
        agent_observers,
        agent_slots,
        player_name,
        show_typestate_graph,
        guards,
    )
    .await;
}

/// Background task: polls game server state and writes to shared view state.
#[instrument(skip_all)]
async fn blackjack_poll_task(
    state: Arc<RwLock<BlackjackViewState>>,
    mut action_rx: tokio::sync::mpsc::Receiver<BlackjackAction>,
    human: crate::tui::rest_client::HumanBlackjackClient,
    human_observer: crate::tui::rest_client::BlackjackObserver,
    agent_observers: Vec<crate::tui::rest_client::BlackjackObserver>,
    agent_slots: Vec<crate::PlayerSlot>,
    player_name: String,
    _show_typestate_graph: bool,
    _guards: crate::tui::standalone::ProcessGuards,
) {
    use crate::session::SharedTableSeatView;
    use crate::tui::blackjack::phase_transition_story;
    use crate::tui::typestate_widget::blackjack_active;
    use tokio::time::Duration;

    let idle_state = SharedTableSeatView {
        phase: "idle".to_string(),
        bankroll: 0,
        description: "Connecting…".to_string(),
        is_terminal: false,
        player_hands: vec![],
        dealer_hand: vec![],
    };

    let mut prev_human_phase = "idle".to_string();
    let mut prev_agent_phases: Vec<String> = vec!["idle".to_string(); agent_observers.len()];
    let mut agent_dialogues: Vec<Vec<DialogueEntry>> = vec![Vec::new(); agent_slots.len()];
    let mut tool_refresh_counter: u8 = 0;

    loop {
        // ── Drain any pending player actions ─────────────────────────────────
        while let Ok(action) = action_rx.try_recv() {
            match action {
                BlackjackAction::CallTool { name, args } => {
                    let _ = human.call_tool(&name, args).await;
                    // Clear tools immediately so the UI shows "waiting" state.
                    if let Ok(mut s) = state.write() {
                        s.available_tools.clear();
                        s.tool_descs.clear();
                    }
                }
                BlackjackAction::Quit => {
                    if let Ok(mut s) = state.write() {
                        s.outcome = Some(BlackjackSessionOutcome::Abandoned);
                    }
                    return;
                }
            }
        }

        // ── Poll state from server ────────────────────────────────────────────
        let human_state = human_observer
            .get_blackjack_state()
            .await
            .unwrap_or_else(|_| idle_state.clone());

        let mut agent_states: Vec<SharedTableSeatView> = Vec::with_capacity(agent_observers.len());
        for (i, obs) in agent_observers.iter().enumerate() {
            let s = obs
                .get_blackjack_state()
                .await
                .unwrap_or_else(|_| idle_state.clone());
            agent_states.push(s);
            if let Ok(entries) = obs.get_dialogue().await {
                agent_dialogues[i] = entries;
            }
        }

        tool_refresh_counter = tool_refresh_counter.wrapping_add(1);
        let (available_tools, tool_descs) = if tool_refresh_counter.is_multiple_of(2) {
            match human.list_blackjack_tools().await {
                Ok(tools) => {
                    let descs = tools.iter().map(|t| t.description.clone()).collect();
                    (tools, descs)
                }
                Err(_) => (vec![], vec![]),
            }
        } else {
            let s = state.read().unwrap();
            (s.available_tools.clone(), s.tool_descs.clone())
        };

        // ── Detect terminal outcome ───────────────────────────────────────────
        let outcome = if human_state.is_terminal {
            let bankroll = human_state.bankroll;
            let desc = human_state.description.to_lowercase();
            Some(if desc.contains("win") || desc.contains("blackjack") {
                BlackjackSessionOutcome::Win(bankroll)
            } else if desc.contains("push") {
                BlackjackSessionOutcome::Push(bankroll)
            } else {
                BlackjackSessionOutcome::Loss(bankroll)
            })
        } else {
            None
        };

        // ── Update event log on phase transitions ─────────────────────────────
        let mut event_log = state.read().unwrap().event_log.clone();
        if human_state.phase != prev_human_phase {
            event_log.push(phase_transition_story(
                &player_name,
                &prev_human_phase,
                &human_state.phase,
                &human_state.description,
            ));
            prev_human_phase = human_state.phase.clone();
        }
        for (i, (s, prev)) in agent_states
            .iter()
            .zip(prev_agent_phases.iter_mut())
            .enumerate()
        {
            if s.phase != *prev {
                let name = agent_slots
                    .get(i)
                    .map(|a| a.name.as_str())
                    .unwrap_or("Agent");
                event_log.push(phase_transition_story(name, prev, &s.phase, &s.description));
                *prev = s.phase.clone();
            }
        }

        // ── Write updated state ───────────────────────────────────────────────
        let bj_view = BlackjackStateView {
            phase: human_state.phase.clone(),
            bankroll: human_state.bankroll,
            description: human_state.description.clone(),
            is_terminal: human_state.is_terminal,
            player_hands: human_state.player_hands.clone(),
            dealer_hand: human_state.dealer_hand.clone(),
        };

        let agent_triples: Vec<(String, String, String)> = agent_slots
            .iter()
            .zip(agent_states.iter())
            .map(|(slot, s)| (slot.name.clone(), s.phase.clone(), s.description.clone()))
            .collect();

        let merged_dialogue: Vec<DialogueEntry> = agent_dialogues
            .iter()
            .zip(agent_slots.iter())
            .flat_map(|(d, slot)| {
                d.iter().map(move |e| DialogueEntry {
                    role: format!("{} ({})", e.role, slot.name),
                    text: e.text.clone(),
                })
            })
            .collect();

        let active_node = blackjack_active(&human_state.phase);

        if let Ok(mut s) = state.write() {
            s.bj_view = bj_view;
            s.agent_triples = agent_triples;
            s.event_log = event_log;
            s.merged_dialogue = merged_dialogue;
            s.tool_descs = tool_descs;
            s.available_tools = available_tools;
            s.active_node = active_node;
            if outcome.is_some() {
                s.outcome = outcome;
            }
        }

        if outcome.is_some() {
            return;
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// ── TTT session ───────────────────────────────────────────────────────────────

/// All state needed to render one TTT frame.
#[derive(Debug, Clone)]
pub struct TttViewState {
    /// Current board state.
    pub game: AnyGame,
    /// Human player's cursor position.
    pub cursor: Position,
    /// Ordered story entries for the Game Story panel.
    pub event_log: Vec<GameEvent>,
    /// Agent dialogue for the Chat panel.
    pub dialogue: Vec<DialogueEntry>,
    /// Which mark the human player holds.
    pub human_mark: TicTacToePlayer,
    /// True when the game is over.
    pub is_over: bool,
}

impl Default for TttViewState {
    fn default() -> Self {
        Self {
            game: AnyGame::InProgress {
                board: crate::Board::default(),
                to_move: TicTacToePlayer::X,
                history: Vec::new(),
            },
            cursor: Position::Center,
            event_log: vec![GameEvent::story(
                "🎮 Connecting to game server…".to_string(),
            )],
            dialogue: vec![],
            human_mark: TicTacToePlayer::X,
            is_over: false,
        }
    }
}

/// Actions a frontend can send to the TTT session task.
#[derive(Debug)]
pub enum TttAction {
    /// Move the cursor in the given direction.
    MoveCursor(Position),
    /// Place a mark at the current cursor position.
    PlaceMove,
    /// Player quit the session.
    Quit,
}

/// Handle to a running TTT session.
pub struct TttSessionHandle {
    /// Shared session state — read-lock each frame to render.
    pub state: Arc<RwLock<TttViewState>>,
    /// Send player actions to the session task.
    pub action_tx: tokio::sync::mpsc::Sender<TttAction>,
}

/// Start a TTT session and return a handle the frontend can render from.
///
/// Returns immediately — the handle's shared state starts in "connecting" mode
/// while a background tokio task does the async server/client/agent setup.
#[instrument(skip_all, fields(port))]
pub fn start_ttt_session(
    agent_config_path: PathBuf,
    player_name: String,
    port: u16,
    first_player: FirstPlayer,
    show_typestate_graph: bool,
) -> TttSessionHandle {
    let state = Arc::new(RwLock::new(TttViewState::default()));
    let (action_tx, action_rx) = tokio::sync::mpsc::channel(32);

    let state_task = state.clone();
    tokio::spawn(ttt_setup_task(
        state_task,
        action_rx,
        agent_config_path,
        player_name,
        port,
        first_player,
        show_typestate_graph,
    ));

    TttSessionHandle { state, action_tx }
}

/// Background task: async setup then poll loop for TTT.
#[instrument(skip_all)]
async fn ttt_setup_task(
    state: Arc<RwLock<TttViewState>>,
    action_rx: tokio::sync::mpsc::Receiver<TttAction>,
    agent_config_path: PathBuf,
    player_name: String,
    port: u16,
    first_player: FirstPlayer,
    show_typestate_graph: bool,
) {
    use crate::tui::rest_client::RestGameClient;
    use crate::tui::standalone::{GameMode, ProcessGuards, spawn_agent, spawn_server};

    // Use the requested port if free; fall back to an OS-allocated port only
    // if the requested port is already bound.
    let port = ensure_free_port(port);
    tracing::debug!(port, "TTT session port selected");

    let server_url = format!("http://localhost:{port}");

    let server = match spawn_server(port).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "Failed to spawn TTT server");
            return;
        }
    };

    let (client, guards) = match first_player {
        FirstPlayer::Human => {
            let client =
                match RestGameClient::register(server_url, "tui_session".to_string(), player_name)
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to register TTT client");
                        return;
                    }
                };
            let agent = match spawn_agent(port, agent_config_path, GameMode::TicTacToe).await {
                Ok(a) => a,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to spawn TTT agent");
                    return;
                }
            };
            (client, ProcessGuards::new(server, agent))
        }
        FirstPlayer::Agent => {
            let agent = match spawn_agent(port, agent_config_path, GameMode::TicTacToe).await {
                Ok(a) => a,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to spawn TTT agent");
                    return;
                }
            };
            let client =
                match RestGameClient::register(server_url, "tui_session".to_string(), player_name)
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to register TTT client");
                        return;
                    }
                };
            (client, ProcessGuards::new(server, agent))
        }
    };

    let human_mark = client.player_mark;
    if let Ok(mut s) = state.write() {
        s.human_mark = human_mark;
        s.event_log = vec![GameEvent::story(
            "🎮 Game begins — X moves first".to_string(),
        )];
    }

    ttt_poll_task(state, action_rx, client, guards, show_typestate_graph).await;
}

/// Background task: polls TTT game state and writes to shared view state.
#[instrument(skip_all)]
async fn ttt_poll_task(
    state: Arc<RwLock<TttViewState>>,
    mut action_rx: tokio::sync::mpsc::Receiver<TttAction>,
    mut client: crate::tui::rest_client::RestGameClient,
    _guards: crate::tui::standalone::ProcessGuards,
    _show_typestate_graph: bool,
) {
    use crate::games::tictactoe::Player;
    use tokio::time::Duration;

    let mut prev_move_count: usize = 0;
    let mut prev_phase: Option<&'static str> = None;

    loop {
        // ── Drain player actions ──────────────────────────────────────────────
        let cursor = state.read().unwrap().cursor;
        while let Ok(action) = action_rx.try_recv() {
            match action {
                TttAction::PlaceMove => {
                    if let Err(e) = client.make_move(cursor).await {
                        tracing::warn!(error = %e, "make_move failed");
                    }
                }
                TttAction::MoveCursor(pos) => {
                    if let Ok(mut s) = state.write() {
                        s.cursor = pos;
                    }
                }
                TttAction::Quit => {
                    if let Ok(mut s) = state.write() {
                        s.is_over = true;
                    }
                    return;
                }
            }
        }

        // ── Poll game state ───────────────────────────────────────────────────
        let game = match client.get_game().await {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!(error = %e, "get_game failed");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };

        let dialogue = client.get_dialogue().await.unwrap_or_default();

        // ── Build event log ───────────────────────────────────────────────────
        let mut event_log = state.read().unwrap().event_log.clone();

        let current_phase = super::tictactoe_phase_name(&game);
        if Some(current_phase) != prev_phase {
            if let Some(prev) = prev_phase {
                event_log.push(GameEvent::phase_change(prev, current_phase));
            }
            if game.is_over() {
                if let Some(winner) = game.winner() {
                    let w = if winner == Player::X { "X" } else { "O" };
                    event_log.push(GameEvent::result(format!("🏆 {} wins!", w)));
                } else {
                    event_log.push(GameEvent::result("🤝 Draw — the board is full"));
                }
            }
            prev_phase = Some(current_phase);
        }

        let history = game.history();
        for (i, &pos) in history.iter().enumerate().skip(prev_move_count) {
            let player = if i % 2 == 0 { "X" } else { "O" };
            event_log.push(GameEvent::story(format!(
                "  {} {} plays {}",
                if player == "X" { "✕" } else { "◯" },
                player,
                pos.label(),
            )));
            event_log.push(GameEvent::proof("LegalMove"));
        }
        prev_move_count = history.len();

        // ── Write updated state ───────────────────────────────────────────────
        let is_over = game.is_over();
        if let Ok(mut s) = state.write() {
            s.game = game;
            s.event_log = event_log;
            s.dialogue = dialogue;
            s.is_over = is_over;
        }

        if is_over {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Ok(mut s) = state.write() {
                s.is_over = true;
            }
            return;
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
