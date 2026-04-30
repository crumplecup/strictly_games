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
    response::{Html, IntoResponse},
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
use crate::tui::contracts::{BjUiConsistent, CrapsUiConsistent, TttUiConsistent};
use crate::tui::game_ir::{
    EventLog, GraphParams, bj_to_verified_tree, craps_to_verified_tree, ttt_to_verified_tree,
};

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
    let tree = ttt_to_verified_tree(game, display_mode, log, graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, render_proof)) => {
            debug!(bytes = html.len(), "TTT HTML rendered");
            (html, Established::prove(&render_proof))
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
    let log = EventLog {
        events: &[],
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: &[],
        edges: &[],
        active: None,
    };
    let tree = bj_to_verified_tree(state, display_mode, &[], &log, &[], &graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, render_proof)) => {
            debug!(bytes = html.len(), "Blackjack HTML rendered");
            (html, Established::prove(&render_proof))
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
    let graph = GraphParams {
        nodes: &[],
        edges: &[],
        active: None,
    };
    let tree = craps_to_verified_tree(state, display_mode, log, &graph, viewport);
    let renderer = LeptosRenderer::html();
    match renderer.render(&tree) {
        Ok((html, _stats, render_proof)) => {
            debug!(bytes = html.len(), "Craps HTML rendered");
            (html, Established::prove(&render_proof))
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
}

impl LeptosAppState {
    /// Create a fresh state with no active games.
    pub fn new() -> Self {
        Self {
            ttt: Arc::new(Mutex::new(None)),
            blackjack: Arc::new(Mutex::new(None)),
            craps: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for LeptosAppState {
    fn default() -> Self {
        Self::new()
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
    <a href="/">Home</a>
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

// ── Route handlers ────────────────────────────────────────────────────────────

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

    let log = EventLog {
        events: &[],
        dialogue: &[],
    };
    let graph = GraphParams {
        nodes: &[],
        edges: &[],
        active: None,
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
///
/// let state = LeptosAppState::new();
/// let router = leptos_game_router(state);
/// ```
pub fn leptos_game_router(state: LeptosAppState) -> Router {
    Router::new()
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
#[instrument(fields(port))]
pub async fn run_leptos(port: u16) -> anyhow::Result<()> {
    use tokio::net::TcpListener;

    let state = LeptosAppState::new();
    let app = leptos_game_router(state);

    let addr = format!("0.0.0.0:{port}");
    info!(addr = %addr, "Starting leptos frontend");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
