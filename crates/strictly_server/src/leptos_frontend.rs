//! Leptos/Axum browser frontend for Strictly Games.
//!
//! Exposes HTTP routes backed by the same AccessKit IR pipeline used by the
//! ratatui and egui frontends.  Every HTML response is gated on an
//! [`Established<TttUiConsistent>`] / `BjUiConsistent` / `CrapsUiConsistent`
//! proof token minted by `*_to_verified_tree`, preserving the IR-sourced
//! contract across all three frontends.
//!
//! ## Route summary
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | GET | `/` | Full game-selector page |
//! | GET | `/games/ttt` | Live TTT state as HTML |
//! | POST | `/games/ttt/move` | Submit a TTT move |
//! | GET | `/games/ttt/new` | Reset TTT to a fresh game |
//! | GET | `/games/blackjack` | Current blackjack state as HTML |
//! | GET | `/games/craps` | Current craps state as HTML |

#![cfg(not(kani))]

use std::sync::Arc;

use axum::{
    Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::get,
};
use elicit_leptos::LeptosRenderer;
use elicit_ui::{UiTreeRenderer as _, Viewport};
use elicitation::contracts::Established;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

use strictly_blackjack::BlackjackDisplayMode;
use strictly_craps::CrapsDisplayMode;
use strictly_tictactoe::{Player, Position, TttDisplayMode};

use crate::games::blackjack::BlackjackStateView;
use crate::games::craps::CrapsStateView;
use crate::games::tictactoe::AnyGame;
use crate::lobby::lobby_ir::{
    agent_select_to_verified_tree, main_lobby_to_verified_tree, profile_select_to_verified_tree,
    settings_to_verified_tree, stats_view_to_verified_tree,
};
use crate::lobby::settings::{GameType, LobbySettings};
use crate::tui::contracts::{BjUiConsistent, CrapsUiConsistent, TttUiConsistent};
use crate::tui::game_ir::{
    EventLog, GraphParams, bj_to_verified_tree, craps_to_verified_tree, ttt_to_verified_tree,
};
use crate::tui::{
    blackjack_active, blackjack_edges, blackjack_nodes, craps_active, craps_edges, craps_nodes,
    tictactoe_active, tictactoe_edges, tictactoe_nodes,
};
use crate::{AgentLibrary, ProfileService, User};

// ── IR render helpers ─────────────────────────────────────────────────────────

/// Render a TTT game state to HTML via the WCAG AccessKit IR.
///
/// Returns the HTML fragment and a proof that the full pipeline ran:
/// `game_state → VerifiedTree → WcagVerified → LeptosRenderer → HTML`.
#[instrument(skip(game, log, graph))]
pub fn render_ttt_html(
    game: &AnyGame,
    display_mode: &TttDisplayMode,
    log: &EventLog<'_>,
    graph: &GraphParams<'_>,
    viewport: Viewport,
) -> (String, Established<TttUiConsistent>) {
    use elicitation::contracts::both;
    let (tree, wraps_proof) = ttt_to_verified_tree(game, display_mode, log, graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, render_proof)) => {
            debug!(bytes = html.len(), "TTT HTML rendered");
            (html, Established::prove(&both(render_proof, wraps_proof)))
        }
        Err(e) => {
            error!(error = %e, "LeptosRenderer::render failed for TTT");
            (
                format!("<p class=\"error\">Render error: {e}</p>"),
                Established::assert(),
            )
        }
    }
}

/// Render a Blackjack state view to HTML via the WCAG AccessKit IR.
#[instrument(skip(state))]
pub fn render_bj_html(
    state: &BlackjackStateView,
    display_mode: &BlackjackDisplayMode,
    viewport: Viewport,
) -> (String, Established<BjUiConsistent>) {
    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();
    let log = EventLog {
        events: &[],
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: &bj_nodes,
        edges: &bj_edges,
        active: blackjack_active(&state.phase),
    };
    let (tree, _display_proof, _wraps_proof) =
        bj_to_verified_tree(state, display_mode, &[], &log, &[], &graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, _render_proof)) => {
            debug!(bytes = html.len(), "Blackjack HTML rendered");
            (html, Established::assert())
        }
        Err(e) => {
            error!(error = %e, "LeptosRenderer::render failed for Blackjack");
            (
                format!("<p class=\"error\">Render error: {e}</p>"),
                Established::assert(),
            )
        }
    }
}

/// Render a Craps state view to HTML via the WCAG AccessKit IR.
#[instrument(skip(state))]
pub fn render_craps_html(
    state: &CrapsStateView,
    display_mode: &CrapsDisplayMode,
    log: &EventLog<'_>,
    viewport: Viewport,
) -> (String, Established<CrapsUiConsistent>) {
    let craps_ns = craps_nodes();
    let craps_es = craps_edges();
    let graph = GraphParams {
        nodes: &craps_ns,
        edges: &craps_es,
        active: craps_active(&state.phase),
    };
    use elicitation::contracts::both;
    let (tree, wraps_proof) = craps_to_verified_tree(state, display_mode, log, &graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, render_proof)) => {
            debug!(bytes = html.len(), "Craps HTML rendered");
            (html, Established::prove(&both(render_proof, wraps_proof)))
        }
        Err(e) => {
            error!(error = %e, "LeptosRenderer::render failed for Craps");
            (
                format!("<p class=\"error\">Render error: {e}</p>"),
                Established::assert(),
            )
        }
    }
}

// ── Shared axum state ─────────────────────────────────────────────────────────

/// Shared state for the leptos HTTP frontend.
#[derive(Clone)]
pub struct LeptosAppState {
    /// Active TTT game (None = no game started).
    pub ttt: Arc<Mutex<Option<AnyGame>>>,
    /// Latest blackjack snapshot (None = no session).
    pub blackjack: Arc<Mutex<Option<BlackjackStateView>>>,
    /// Latest craps snapshot (None = no session).
    pub craps: Arc<Mutex<Option<CrapsStateView>>>,

    // ── Lobby state ───────────────────────────────────────────────────────────
    /// Profile service for user accounts and stats.
    pub profile_service: ProfileService,
    /// Available AI agent configurations.
    pub agent_library: AgentLibrary,
    /// Currently logged-in user (None = profile not yet selected).
    pub current_user: Arc<Mutex<Option<User>>>,
    /// Selected game type.
    pub selected_game: Arc<Mutex<GameType>>,
    /// Lobby preferences.
    pub lobby_settings: Arc<Mutex<LobbySettings>>,
}

impl LeptosAppState {
    /// Create a fresh state backed by the given lobby services.
    pub fn new_with_lobby(profile_service: ProfileService, agent_library: AgentLibrary) -> Self {
        Self {
            ttt: Arc::new(Mutex::new(None)),
            blackjack: Arc::new(Mutex::new(None)),
            craps: Arc::new(Mutex::new(None)),
            profile_service,
            agent_library,
            current_user: Arc::new(Mutex::new(None)),
            selected_game: Arc::new(Mutex::new(GameType::default())),
            lobby_settings: Arc::new(Mutex::new(LobbySettings::default())),
        }
    }
}

// ── Request/response helpers ──────────────────────────────────────────────────

/// Default viewport for HTTP responses (HD landscape).
fn default_viewport() -> Viewport {
    Viewport::new(1280, 720)
}

fn html_page(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>{title}</title>
  <style>
    body {{ font-family: monospace; background: #1e1e2e; color: #cdd6f4; margin: 2rem; }}
    nav a {{ color: #89b4fa; margin-right: 1rem; }}
    .error {{ color: #f38ba8; }}
    .status {{ color: #a6e3a1; }}
  </style>
</head>
<body>
  <nav>
    <a href="/lobby">Lobby</a>
    <a href="/games/ttt">Tic-tac-toe</a>
    <a href="/games/blackjack">Blackjack</a>
    <a href="/games/craps">Craps</a>
  </nav>
  <hr/>
  {body}
</body>
</html>"#
    )
}

// ── Lobby route helpers ───────────────────────────────────────────────────────

/// Render a `VerifiedTree` to an HTML fragment via `LeptosRenderer`.
fn render_lobby_html(tree: elicit_ui::VerifiedTree) -> String {
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, _proof)) => html,
        Err(e) => {
            error!(error = %e, "LeptosRenderer failed for lobby screen");
            format!("<p class='error'>Render error: {e}</p>")
        }
    }
}

/// Helper: load users from profile service (blocking).
fn load_users(state: &LeptosAppState) -> Vec<User> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| handle.block_on(state.profile_service.repository().list_users()))
        .unwrap_or_default()
}

// ── Lobby route handlers ──────────────────────────────────────────────────────

/// `GET /lobby` — profile selection page.
#[instrument(skip(state))]
async fn handle_lobby(State(state): State<LeptosAppState>) -> Html<String> {
    let users = load_users(&state);
    let selected = state.current_user.lock().await;
    let selected_idx = selected
        .as_ref()
        .and_then(|u| users.iter().position(|user| user.id() == u.id()));
    drop(selected);

    let tree =
        profile_select_to_verified_tree(&users, selected_idx, false, "", None, default_viewport());
    let fragment = render_lobby_html(tree);

    let mut links = String::from("<ul>");
    for user in &users {
        links.push_str(&format!(
            "<li><a href='/lobby/select?id={}'>{}</a></li>",
            user.id(),
            user.display_name()
        ));
    }
    links.push_str("</ul>");
    let create_form = r#"<form method="get" action="/lobby/create">
  <label>New profile: <input name="name" required/></label>
  <button type="submit">Create</button>
</form>"#;

    Html(html_page(
        "Select Profile",
        &format!("{fragment}{links}{create_form}"),
    ))
}

/// `GET /lobby/select?id=USER_ID` — select an existing profile.
#[derive(Debug, Deserialize)]
struct SelectParams {
    id: String,
}
#[instrument(skip(state))]
async fn handle_lobby_select(
    State(state): State<LeptosAppState>,
    Query(p): Query<SelectParams>,
) -> impl IntoResponse {
    let handle = tokio::runtime::Handle::current();
    let user = tokio::task::block_in_place(|| {
        handle.block_on(state.profile_service.repository().get_user_by_id(&p.id))
    })
    .ok()
    .flatten();
    if let Some(user) = user {
        *state.current_user.lock().await = Some(user);
    }
    Redirect::to("/lobby/main")
}

/// `GET /lobby/create?name=NAME` — create a new profile and select it.
#[derive(Debug, Deserialize)]
struct CreateParams {
    name: String,
}
#[instrument(skip(state))]
async fn handle_lobby_create(
    State(state): State<LeptosAppState>,
    Query(p): Query<CreateParams>,
) -> impl IntoResponse {
    let handle = tokio::runtime::Handle::current();
    let user = tokio::task::block_in_place(|| {
        handle.block_on(state.profile_service.get_or_create_user(p.name))
    })
    .ok();
    if let Some(user) = user {
        *state.current_user.lock().await = Some(user);
    }
    Redirect::to("/lobby/main")
}

/// `GET /lobby/main` — main lobby menu.
#[instrument(skip(state))]
async fn handle_lobby_main(State(state): State<LeptosAppState>) -> Html<String> {
    let user = state.current_user.lock().await.clone();
    let Some(user) = user else {
        return Html(html_page(
            "Lobby",
            "<p>No profile selected. <a href='/lobby'>Choose one here</a>.</p>",
        ));
    };
    let game = *state.selected_game.lock().await;
    let handle = tokio::runtime::Handle::current();
    let stats =
        tokio::task::block_in_place(|| handle.block_on(state.profile_service.get_stats(user.id())))
            .ok();

    let tree = main_lobby_to_verified_tree(&user, stats.as_ref(), game, 0, default_viewport());
    let fragment = render_lobby_html(tree);

    let actions = format!(
        r#"<ul>
  <li><a href='/lobby/agents'>Play Game ({game})</a></li>
  <li><a href='/lobby/game-select'>Select Game</a></li>
  <li><a href='/lobby/stats'>View Statistics</a></li>
  <li><a href='/lobby'>Change Profile</a></li>
  <li><a href='/lobby/settings'>Settings</a></li>
</ul>"#,
        game = game.label()
    );
    Html(html_page(
        "Strictly Games — Lobby",
        &format!("{fragment}{actions}"),
    ))
}

/// `GET /lobby/game-select` — game selection page.
#[instrument(skip(state))]
async fn handle_lobby_game_select(State(state): State<LeptosAppState>) -> Html<String> {
    let current = *state.selected_game.lock().await;
    let idx = GameType::all()
        .iter()
        .position(|&g| g == current)
        .unwrap_or(0);
    let tree = crate::lobby::lobby_ir::game_select_to_verified_tree(idx, default_viewport());
    let fragment = render_lobby_html(tree);

    let links: String = GameType::all()
        .iter()
        .map(|g| {
            format!(
                "<li><a href='/lobby/game-select/set?game={}'>{}</a></li>",
                g.id(),
                g.label()
            )
        })
        .collect::<Vec<_>>()
        .join("");
    Html(html_page(
        "Select Game",
        &format!("{fragment}<ul>{links}</ul>"),
    ))
}

/// `GET /lobby/game-select/set?game=GAME_ID` — set selected game.
#[derive(Debug, Deserialize)]
struct SetGameParams {
    game: String,
}
#[instrument(skip(state))]
async fn handle_lobby_set_game(
    State(state): State<LeptosAppState>,
    Query(p): Query<SetGameParams>,
) -> impl IntoResponse {
    let game = match p.game.as_str() {
        "blackjack" => GameType::Blackjack,
        "craps" => GameType::Craps,
        _ => GameType::TicTacToe,
    };
    *state.selected_game.lock().await = game;
    Redirect::to("/lobby/main")
}

/// `GET /lobby/settings` — settings page.
#[instrument(skip(state))]
async fn handle_lobby_settings(State(state): State<LeptosAppState>) -> Html<String> {
    let settings = *state.lobby_settings.lock().await;
    let tree = settings_to_verified_tree(&settings, 0, default_viewport());
    let fragment = render_lobby_html(tree);

    let actions = format!(
        r#"<ul>
  <li>Who Goes First: {} <a href='/lobby/settings/toggle?s=first_player'>[toggle]</a></li>
  <li>Show Typestate Graph: {} <a href='/lobby/settings/toggle?s=typestate_graph'>[toggle]</a></li>
</ul>
<a href='/lobby/main'>Back to Lobby</a>"#,
        settings.first_player.label(),
        if settings.show_typestate_graph {
            "✓"
        } else {
            "○"
        }
    );
    Html(html_page("Settings", &format!("{fragment}{actions}")))
}

/// `GET /lobby/settings/toggle?s=SETTING` — toggle a setting.
#[derive(Debug, Deserialize)]
struct ToggleParams {
    s: String,
}
#[instrument(skip(state))]
async fn handle_lobby_settings_toggle(
    State(state): State<LeptosAppState>,
    Query(p): Query<ToggleParams>,
) -> impl IntoResponse {
    let mut settings = state.lobby_settings.lock().await;
    match p.s.as_str() {
        "first_player" => settings.first_player = settings.first_player.toggle(),
        "typestate_graph" => settings.show_typestate_graph = !settings.show_typestate_graph,
        _ => {}
    }
    Redirect::to("/lobby/settings")
}

/// `GET /lobby/stats` — stats view.
#[instrument(skip(state))]
async fn handle_lobby_stats(State(state): State<LeptosAppState>) -> Html<String> {
    let user = state.current_user.lock().await.clone();
    let Some(user) = user else {
        return Html(html_page(
            "Stats",
            "<p>No profile selected. <a href='/lobby'>Choose one here</a>.</p>",
        ));
    };
    let handle = tokio::runtime::Handle::current();
    let aggregated =
        tokio::task::block_in_place(|| handle.block_on(state.profile_service.get_stats(user.id())))
            .ok();
    let recent_games = tokio::task::block_in_place(|| {
        handle.block_on(state.profile_service.get_history(user.id()))
    })
    .unwrap_or_default();

    let tree = stats_view_to_verified_tree(
        &user,
        aggregated.as_ref(),
        &recent_games,
        default_viewport(),
    );
    let fragment = render_lobby_html(tree);
    Html(html_page(
        &format!("Statistics — {}", user.display_name()),
        &format!("{fragment}<p><a href='/lobby/main'>Back to Lobby</a></p>"),
    ))
}

/// `GET /lobby/agents` — agent selection page.
#[instrument(skip(state))]
async fn handle_lobby_agents(State(state): State<LeptosAppState>) -> Html<String> {
    let agents = state.agent_library.agents().to_vec();
    let tree = agent_select_to_verified_tree(&agents, None, default_viewport());
    let fragment = render_lobby_html(tree);

    let links: String = agents
        .iter()
        .map(|a| {
            format!(
                "<li><a href='/lobby/play?agent={}'>{} ({:?} / {})</a></li>",
                a.name(),
                a.name(),
                a.llm_provider(),
                a.llm_model()
            )
        })
        .collect::<Vec<_>>()
        .join("");
    Html(html_page(
        "Select Agent",
        &format!("{fragment}<ul>{links}</ul><p><a href='/lobby/main'>Back</a></p>"),
    ))
}

/// `GET /lobby/play?agent=AGENT_NAME` — start a game with an agent.
#[derive(Debug, Deserialize)]
struct PlayParams {
    agent: String,
}
#[instrument(skip(state))]
async fn handle_lobby_play(
    State(state): State<LeptosAppState>,
    Query(p): Query<PlayParams>,
) -> impl IntoResponse {
    let game = *state.selected_game.lock().await;
    info!(agent = %p.agent, game = %game.label(), "Starting game from lobby");
    // Route to appropriate game page
    let redirect = match game {
        GameType::TicTacToe => "/games/ttt/new",
        GameType::Blackjack => "/games/blackjack",
        GameType::Craps => "/games/craps",
    };
    Redirect::to(redirect)
}

// ── Game route handlers ───────────────────────────────────────────────────────

/// `GET /` — game selection page.
#[instrument(skip(_state))]
async fn handle_home(_state: State<LeptosAppState>) -> Html<String> {
    let body = r#"
<h1>Strictly Games</h1>
<ul>
  <li><a href="/games/ttt/new">New Tic-tac-toe game</a></li>
  <li><a href="/games/blackjack">Blackjack (spectator)</a></li>
  <li><a href="/games/craps">Craps (spectator)</a></li>
</ul>
"#;
    Html(html_page("Strictly Games", body))
}

/// `GET /games/ttt` — render current TTT game state as HTML.
#[instrument(skip(state))]
async fn handle_ttt(State(state): State<LeptosAppState>) -> Html<String> {
    let lock = state.ttt.lock().await;
    let game = match lock.as_ref() {
        Some(g) => g,
        None => {
            let body = r#"<p>No active game. <a href="/games/ttt/new">Start one?</a></p>"#;
            return Html(html_page("Tic-tac-toe", body));
        }
    };

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let log = EventLog {
        events: &[],
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: &ttt_nodes,
        edges: &ttt_edges,
        active: tictactoe_active(game),
    };
    let (fragment, _proof) = render_ttt_html(
        game,
        &TttDisplayMode::Board,
        &log,
        &graph,
        default_viewport(),
    );

    let controls = r#"
<form method="get" action="/games/ttt/move" style="margin-top:1rem">
  <label>Position (0–8):
    <input type="number" name="pos" min="0" max="8" required/>
  </label>
  <button type="submit">Play</button>
</form>
<p><a href="/games/ttt/new">New game</a></p>
"#;
    Html(html_page("Tic-tac-toe", &format!("{fragment}{controls}")))
}

/// `GET /games/ttt/new` — reset TTT to a fresh game and redirect.
#[instrument(skip(state))]
async fn handle_ttt_new(State(state): State<LeptosAppState>) -> impl IntoResponse {
    let mut lock = state.ttt.lock().await;
    *lock = Some(AnyGame::InProgress {
        board: strictly_tictactoe::Board::default(),
        to_move: strictly_tictactoe::Player::X,
        history: Vec::new(),
    });
    info!("New TTT game started");
    axum::response::Redirect::to("/games/ttt")
}

/// Query params for `GET /games/ttt/move`.
#[derive(Deserialize)]
struct TttMoveParams {
    pos: u8,
}

/// `GET /games/ttt/move?pos=N` — attempt a move and redirect back.
#[instrument(skip(state, params))]
async fn handle_ttt_move(
    State(state): State<LeptosAppState>,
    Query(params): Query<TttMoveParams>,
) -> impl IntoResponse {
    let position = position_from_index(params.pos);
    let mut lock = state.ttt.lock().await;
    if let Some(game) = lock.as_ref() {
        let mover = game.to_move().unwrap_or(Player::X);
        match game
            .clone()
            .make_move_action(strictly_tictactoe::action::Move::new(mover, position))
        {
            Ok(next) => {
                info!(pos = params.pos, "TTT move applied");
                *lock = Some(next);
            }
            Err(e) => {
                error!(error = %e, "Illegal TTT move");
            }
        }
    }
    drop(lock);
    axum::response::Redirect::to("/games/ttt")
}

/// `GET /games/blackjack` — render latest blackjack snapshot as HTML.
#[instrument(skip(state))]
async fn handle_blackjack(State(state): State<LeptosAppState>) -> Html<String> {
    let lock = state.blackjack.lock().await;
    let view = match lock.as_ref() {
        Some(v) => v,
        None => {
            let body = "<p class=\"status\">No active blackjack session.</p>";
            return Html(html_page("Blackjack", body));
        }
    };

    let (fragment, _proof) = render_bj_html(view, &BlackjackDisplayMode::Table, default_viewport());
    Html(html_page("Blackjack", &fragment))
}

/// `GET /games/craps` — render latest craps snapshot as HTML.
#[instrument(skip(state))]
async fn handle_craps(State(state): State<LeptosAppState>) -> Html<String> {
    let lock = state.craps.lock().await;
    let view = match lock.as_ref() {
        Some(v) => v,
        None => {
            let body = "<p class=\"status\">No active craps session.</p>";
            return Html(html_page("Craps", body));
        }
    };

    let log = EventLog {
        events: &[],
        dialogue: &[],
    };
    let (fragment, _proof) =
        render_craps_html(view, &CrapsDisplayMode::Table, &log, default_viewport());
    Html(html_page("Craps", &fragment))
}

// ── Position helper ───────────────────────────────────────────────────────────

fn position_from_index(idx: u8) -> Position {
    match idx {
        0 => Position::TopLeft,
        1 => Position::TopCenter,
        2 => Position::TopRight,
        3 => Position::MiddleLeft,
        4 => Position::Center,
        5 => Position::MiddleRight,
        6 => Position::BottomLeft,
        7 => Position::BottomCenter,
        _ => Position::BottomRight,
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Build the axum [`Router`] for the leptos game frontend.
///
/// Mount this under any path prefix (e.g. `.nest("/web", leptos_game_router(state))`)
/// or at the root.
///
/// # Example
///
/// ```rust,no_run
/// use strictly_server::{LeptosAppState, leptos_game_router};
/// use strictly_server::{AgentLibrary, GameRepository, ProfileService};
///
/// let repo = GameRepository::in_memory().expect("in-memory repo");
/// let profile_service = ProfileService::new(repo);
/// let agent_library = AgentLibrary::empty();
/// let state = LeptosAppState::new_with_lobby(profile_service, agent_library);
/// let router = leptos_game_router(state);
/// ```
pub fn leptos_game_router(state: LeptosAppState) -> Router {
    Router::new()
        // ── Lobby ──────────────────────────────────────────────────────────
        .route("/lobby", get(handle_lobby))
        .route("/lobby/select", get(handle_lobby_select))
        .route("/lobby/create", get(handle_lobby_create))
        .route("/lobby/main", get(handle_lobby_main))
        .route("/lobby/game-select", get(handle_lobby_game_select))
        .route("/lobby/game-select/set", get(handle_lobby_set_game))
        .route("/lobby/settings", get(handle_lobby_settings))
        .route("/lobby/settings/toggle", get(handle_lobby_settings_toggle))
        .route("/lobby/stats", get(handle_lobby_stats))
        .route("/lobby/agents", get(handle_lobby_agents))
        .route("/lobby/play", get(handle_lobby_play))
        // ── Games ──────────────────────────────────────────────────────────
        .route("/", get(handle_home))
        .route("/games/ttt", get(handle_ttt))
        .route("/games/ttt/new", get(handle_ttt_new))
        .route("/games/ttt/move", get(handle_ttt_move))
        .route("/games/blackjack", get(handle_blackjack))
        .route("/games/craps", get(handle_craps))
        .with_state(state)
}

/// Run the leptos frontend as a standalone HTTP server on the given port.
///
/// Binds to `0.0.0.0:{port}` and serves the game frontend.  Returns when
/// the server shuts down.
///
/// # Errors
///
/// Returns an error if the address cannot be bound or if the server task fails.
#[instrument(skip(profile_service, agent_library), fields(port))]
pub async fn run_leptos(
    profile_service: ProfileService,
    agent_library: AgentLibrary,
    port: u16,
) -> anyhow::Result<()> {
    use tokio::net::TcpListener;

    let state = LeptosAppState::new_with_lobby(profile_service, agent_library);
    let app = leptos_game_router(state);

    let addr = format!("0.0.0.0:{port}");
    info!(addr = %addr, "Starting leptos frontend");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
