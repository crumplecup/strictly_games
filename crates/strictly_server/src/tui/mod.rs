//! Terminal UI for Strictly Games

#![warn(missing_docs)]

pub mod blackjack;
pub mod chat_widget;
pub mod contextual_communicator;
pub mod contracts;
pub mod craps;
pub mod game_ir;
mod input; // Cursor movement
pub mod mcp_communicator;
pub mod observable_communicator;
mod rest_client; // Type-safe REST client
mod standalone;
pub mod tui_communicator;
mod typestate_widget;

pub use blackjack::{BlackjackSessionOutcome, run_blackjack_mcp_session};
pub use craps::{CrapsCoPlayer, CrapsSessionOutcome, run_craps_session, run_multi_craps_session};
pub use typestate_widget::{EdgeDef, GameEvent, NodeDef};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, path::PathBuf};
use tokio::process::Child;
use tracing::{error, info, instrument};

use crate::{AnyGame, FirstPlayer, TicTacToePlayer};

use crate::games::tictactoe::Position;
use rest_client::RestGameClient;
use typestate_widget::{tictactoe_active, tictactoe_edges, tictactoe_nodes, tictactoe_phase_name};

/// Run the TUI client
#[instrument(skip_all, fields(server_url = ?server_url, port, agent_config = %agent_config.display()))]
pub async fn run(server_url: Option<String>, port: u16, agent_config: PathBuf) -> Result<()> {
    // Setup logging to file to avoid interfering with TUI
    let log_file = std::fs::File::create("strictly_games_tui.log")?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::sync::Arc::new(log_file))
        .with_ansi(false)
        .try_init(); // Don't panic if already initialized

    info!("Starting Strictly Games TUI");

    let session_id = "tui_session".to_string();
    let first_player = FirstPlayer::default(); // Human goes first by default

    // Determine mode: standalone or remote
    let (actual_server_url, server_child, agent_child): (String, Option<Child>, Option<Child>) =
        if let Some(url) = server_url {
            // Remote mode: connect to existing server
            info!(server_url = %url, "Connecting to remote server");
            (url, None, None)
        } else {
            // Standalone mode: spawn server, then register human and agent in configured order.
            info!(port, first_player = %first_player.label(), "Starting standalone mode");
            let server = standalone::spawn_server(port).await?;
            let url = format!("http://localhost:{}", port);
            info!(server_url = %url, "Standalone server ready");
            (url, Some(server), None)
        };

    info!(server_url = %actual_server_url, session_id = %session_id, "Connecting to game server");

    // Register as human player using REST client
    let client =
        match RestGameClient::register(actual_server_url, session_id, "Human".to_string()).await {
            Ok(c) => {
                info!("Successfully registered with server");
                c
            }
            Err(e) => {
                error!(error = %e, "Failed to register with server");
                return Err(e);
            }
        };

    // For standalone mode with default FirstPlayer::Human, spawn agent AFTER human registers
    // so human gets X and moves first
    let _guards = if let (Some(server), None) = (server_child, agent_child) {
        info!("Spawning agent after human registration (human plays X)");
        let agent =
            standalone::spawn_agent(port, agent_config, standalone::GameMode::TicTacToe).await?;
        Some(standalone::ProcessGuards::new(server, agent))
    } else {
        None
    };

    // Setup terminal after server connection succeeds
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    info!("Registered with server, starting game loop");

    // Run type-safe game loop
    let res = run_typesafe_game(&mut terminal, client).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        error!(error = ?err, "Game loop error");
        eprintln!("Error: {:?}", err);
    } else {
        println!("Game completed successfully. Thanks for playing!");
    }

    Ok(())
}

/// Type-safe game loop using REST API.
#[instrument(skip_all, fields(session_id = %client.session_id, player_id = %client.player_id))]
async fn run_typesafe_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut client: RestGameClient, // Make mutable to update last_error
) -> Result<()>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crate::games::tictactoe::Player;
    use crate::session::DialogueEntry;
    use tokio::time::{Duration, sleep};

    info!("Starting type-safe game loop");

    let mut cursor = Position::Center;
    let mut event_log: Vec<GameEvent> = vec![GameEvent::story("🎮 Game begins — X moves first")];
    let mut prev_phase: Option<&'static str> = None;
    let mut prev_move_count: usize = 0;
    let mut dialogue: Vec<DialogueEntry> = Vec::new();
    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();

    loop {
        // Get game state (type-safe!)
        let game = client.get_game().await?;

        // Fetch dialogue (best-effort).
        if let Ok(entries) = client.get_dialogue().await {
            dialogue = entries;
        }

        // Track phase transitions.
        let current_phase = tictactoe_phase_name(&game);
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

        // Track individual moves with rich narration.
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

        let active = tictactoe_active(&game);

        // Render UI
        let graph = GraphState::new(true, &ttt_nodes, &ttt_edges, active);
        let frame_data = FrameData {
            game: &game,
            cursor,
            event_log: &event_log,
            dialogue: &dialogue,
            graph,
        };
        terminal.draw(|f| {
            render_tictactoe_frame(f, &frame_data);
        })?;

        // Handle game over
        if game.is_over()
            && event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    info!("Restarting game");
                    if let Err(e) = client.restart_game().await {
                        error!(error = %e, "Restart failed");
                    }
                    sleep(Duration::from_millis(200)).await; // Let server process
                }
                _ => {}
            }
            sleep(Duration::from_millis(100)).await;
            continue;
        }

        // Handle input
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    // Trigger passive-Affirm escape hatch
                    info!("User pressed 'q', cancelling game");
                    if let Err(e) = client.cancel_game().await {
                        error!(error = %e, "Failed to cancel game");
                    }
                    return Ok(());
                }
                KeyCode::Enter => {
                    info!(position = ?cursor, "Making move");
                    if let Err(e) = client.make_move(cursor).await {
                        error!(error = %e, "Move failed");
                    }
                    sleep(Duration::from_millis(200)).await; // Let server process
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    cursor = input::move_cursor(cursor, key.code);
                }
                _ => {}
            }
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Runs a complete game session from the lobby: spawns server + agent, plays
/// one game to completion, and returns the final game state and the human's mark.
///
/// Unlike [`run`], this does not restart the game or exit on 'q'; pressing any
/// key after the game ends returns the outcome to the lobby controller.
///
/// `first_player` controls registration order: [`FirstPlayer::Human`] registers
/// the human before the agent so the human plays as X and moves first;
/// [`FirstPlayer::Agent`] spawns the agent first so it plays as X.
/// `show_typestate_graph` enables the split-view typestate panel.
#[instrument(skip(terminal), fields(player_name = %player_name, port, first_player = %first_player.label()))]
pub async fn run_game_session<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    agent_config_path: PathBuf,
    player_name: String,
    port: u16,
    first_player: FirstPlayer,
    show_typestate_graph: bool,
) -> Result<(AnyGame, TicTacToePlayer)>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    info!("Starting lobby game session");

    let server_url = format!("http://localhost:{}", port);

    // Spawn server, then register human and agent in the configured order.
    // The first to register gets Mark::X and moves first.
    let server = standalone::spawn_server(port).await?;

    let (client, _guards) = match first_player {
        FirstPlayer::Human => {
            // Register human first → human plays as X (moves first).
            info!("Registering human first (player goes first)");
            let client =
                RestGameClient::register(server_url, "tui_session".to_string(), player_name)
                    .await?;
            let agent =
                standalone::spawn_agent(port, agent_config_path, standalone::GameMode::TicTacToe)
                    .await?;
            (client, standalone::ProcessGuards::new(server, agent))
        }
        FirstPlayer::Agent => {
            // Spawn agent first → agent plays as X (moves first), human gets O.
            info!("Spawning agent first (agent goes first)");
            let agent =
                standalone::spawn_agent(port, agent_config_path, standalone::GameMode::TicTacToe)
                    .await?;
            let client =
                RestGameClient::register(server_url, "tui_session".to_string(), player_name)
                    .await?;
            (client, standalone::ProcessGuards::new(server, agent))
        }
    };

    let human_mark = client.player_mark;
    info!(mark = ?human_mark, "Human player registered");

    // Play one game to completion.
    let final_game = run_lobby_game(terminal, client, show_typestate_graph).await?;
    info!(is_over = final_game.is_over(), "Game session complete");

    Ok((final_game, human_mark))
}

/// Runs a single game to completion.
///
/// Unlike [`run_typesafe_game`], there is no restart ('r') and pressing 'q'
/// or any key after the game ends returns the final [`AnyGame`] to the caller.
#[instrument(skip_all, fields(session_id = %client.session_id, player_id = %client.player_id))]
async fn run_lobby_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut client: RestGameClient,
    show_typestate_graph: bool,
) -> Result<AnyGame>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crate::games::tictactoe::Player;
    use crate::session::DialogueEntry;
    use tokio::time::{Duration, sleep};

    info!(show_typestate_graph, "Starting lobby game loop");

    let mut cursor = Position::Center;
    let mut event_log: Vec<GameEvent> = vec![GameEvent::story("🎮 Game begins — X moves first")];
    let mut prev_phase: Option<&'static str> = None;
    let mut prev_move_count: usize = 0;
    let mut dialogue: Vec<DialogueEntry> = Vec::new();

    loop {
        let game = client.get_game().await?;

        // Fetch dialogue (best-effort, ignore errors).
        if let Ok(entries) = client.get_dialogue().await {
            dialogue = entries;
        }

        // Track phase transitions.
        let current_phase = tictactoe_phase_name(&game);
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

        // Track individual moves with rich narration.
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

        // Once game is over, render final state and wait for any keypress.
        if game.is_over() {
            return render_game_over_and_wait(
                terminal,
                &game,
                cursor,
                &event_log,
                &dialogue,
                show_typestate_graph,
            )
            .await;
        }

        // Render in-progress game.
        {
            let ttt_nodes = tictactoe_nodes();
            let ttt_edges = tictactoe_edges();
            let active = tictactoe_active(&game);
            let graph = GraphState::new(show_typestate_graph, &ttt_nodes, &ttt_edges, active);
            let frame_data = FrameData {
                game: &game,
                cursor,
                event_log: &event_log,
                dialogue: &dialogue,
                graph,
            };
            terminal.draw(|f| {
                render_tictactoe_frame(f, &frame_data);
            })?;
        }

        // Handle input.
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    // Trigger passive-Affirm escape hatch
                    info!("User pressed 'q', cancelling game");
                    if let Err(e) = client.cancel_game().await {
                        error!(error = %e, "Failed to cancel game");
                    }
                    // Fetch and return current game state
                    return client.get_game().await;
                }
                KeyCode::Enter => {
                    info!(position = ?cursor, "Making move");
                    if let Err(e) = client.make_move(cursor).await {
                        error!(error = %e, "Move failed");
                    }
                    sleep(Duration::from_millis(200)).await;
                }
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    cursor = input::move_cursor(cursor, key.code);
                }
                _ => {}
            }
        }

        sleep(Duration::from_millis(50)).await;
    }
}

/// Renders the game-over screen and waits for any keypress.
#[instrument(skip_all)]
async fn render_game_over_and_wait<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    game: &AnyGame,
    cursor: Position,
    event_log: &[GameEvent],
    dialogue: &[crate::session::DialogueEntry],
    show_typestate_graph: bool,
) -> Result<AnyGame>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crossterm::event::KeyEventKind;
    use tokio::time::{Duration, sleep};

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let active = tictactoe_active(game);
    let graph = GraphState::new(show_typestate_graph, &ttt_nodes, &ttt_edges, active);
    let frame_data = FrameData {
        game,
        cursor,
        event_log,
        dialogue,
        graph,
    };
    terminal.draw(|f| {
        render_tictactoe_frame(f, &frame_data);
    })?;

    // Wait for any keypress.
    loop {
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            return Ok(game.clone());
        }
        sleep(Duration::from_millis(50)).await;
    }
}

/// Bundled typestate graph state passed to the frame renderer.
struct GraphState<'a> {
    /// Whether to show the typestate graph column.
    show: bool,
    /// Phase node definitions.
    nodes: &'a [NodeDef],
    /// Edge definitions between nodes.
    edges: &'a [EdgeDef],
    /// Index of the currently active phase node.
    active: Option<usize>,
}

impl<'a> GraphState<'a> {
    /// Creates a new graph state bundle.
    fn new(show: bool, nodes: &'a [NodeDef], edges: &'a [EdgeDef], active: Option<usize>) -> Self {
        Self {
            show,
            nodes,
            edges,
            active,
        }
    }
}

/// All data needed to render a single tic-tac-toe frame.
///
/// Groups parameters that would otherwise cause clippy's
/// `too_many_arguments` lint to fire.
struct FrameData<'a> {
    /// Game state.
    game: &'a AnyGame,
    /// Cursor position for the human player (highlighted in the board IR).
    cursor: Position,
    /// Event log for the story pane.
    event_log: &'a [GameEvent],
    /// Server↔agent dialogue.
    dialogue: &'a [crate::session::DialogueEntry],
    /// Typestate graph state.
    graph: GraphState<'a>,
}

/// Renders the tic-tac-toe frame via the AccessKit IR pipeline.
///
/// Converts game state to a [`VerifiedTree`] via [`ttt_to_verified_tree`],
/// renders it with [`RatatuiBackend`], then draws the resulting [`TuiNode`]
/// tree.  The WCAG credential from [`VerifiedTree::from_parts`] flows through
/// [`RatatuiBackend::render`] → `Established<RenderComplete>`, from which we
/// issue `Established<TttUiConsistent>`.
#[instrument(skip_all)]
fn render_tictactoe_frame(f: &mut ratatui::Frame, data: &FrameData) {
    use crate::tui::contracts::TttUiConsistent;
    use crate::tui::contracts::{NoOverflow, render_resize_prompt, verified_draw};
    use crate::tui::game_ir::{EventLog, GraphParams, ttt_to_verified_tree};
    use elicit_ratatui::RatatuiBackend;
    use elicit_ui::{UiTreeRenderer as _, Viewport};
    use elicitation::contracts::Established;
    use strictly_tictactoe::TttDisplayMode;

    let area = f.area();
    let viewport = Viewport::new(area.width as u32, area.height as u32);

    let empty_nodes: &[_] = &[];
    let empty_edges: &[_] = &[];
    let (graph_nodes, graph_edges) = if data.graph.show {
        (data.graph.nodes, data.graph.edges)
    } else {
        (empty_nodes, empty_edges)
    };
    let log = EventLog {
        events: data.event_log,
        dialogue: data.dialogue,
    };
    let graph = GraphParams {
        nodes: graph_nodes,
        edges: graph_edges,
        active: data.graph.active,
    };

    let tree = ttt_to_verified_tree(
        data.game,
        &TttDisplayMode::BoardWithCursor(data.cursor),
        &log,
        &graph,
        viewport,
    );

    let backend = RatatuiBackend::new();
    let (tui_node, _stats, render_proof) = backend
        .render(&tree)
        .unwrap_or_else(|e| panic!("RatatuiBackend::render failed: {e}"));

    let _proof: Established<NoOverflow> = verified_draw(f, area, &tui_node).unwrap_or_else(|e| {
        render_resize_prompt(f, &e);
        Established::assert()
    });

    // RenderComplete was minted from Established<WcagVerified> inside render(),
    // which was minted from the VerifiedTree.  TttUiConsistent: ProvableFrom<Established<RenderComplete>>
    // threads that guarantee through to the game-level proposition.
    let _ui_proof: Established<TttUiConsistent> = Established::prove(&render_proof);
}
