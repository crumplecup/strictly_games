//! Terminal UI for Strictly Games

#![warn(missing_docs)]

pub mod blackjack;
pub mod chat_widget;
pub mod contextual_communicator;
pub mod contracts;
pub mod craps;
mod input; // Cursor movement
pub mod mcp_communicator;
pub mod observable_communicator;
mod palette;
mod rest_client; // Type-safe REST client
mod standalone;
pub mod tui_communicator;
mod typestate_widget;

pub use blackjack::{BlackjackSessionOutcome, run_blackjack_session, run_multi_blackjack_session};
pub use chat_widget::{ChatMessage, ChatWidget, Participant, chat_channel};
pub use craps::{CrapsCoPlayer, CrapsSessionOutcome, run_craps_session, run_multi_craps_session};
pub use mcp_communicator::LlmElicitCommunicator;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, style::Color};
use std::{io, path::PathBuf};
use tokio::process::Child;
use tracing::{error, info, instrument};

use crate::{AnyGame, FirstPlayer, TicTacToePlayer};

use crate::games::tictactoe::Position;
use rest_client::RestGameClient;
use typestate_widget::{
    EdgeDef, ExploreStats, GameEvent, NodeDef, TypestateGraphWidget, tictactoe_active,
    tictactoe_edges, tictactoe_nodes, tictactoe_phase_name,
};

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
        let agent = standalone::spawn_agent(port, agent_config).await?;
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
    let mut explore_stats = ExploreStats::default();
    let mut dialogue: Vec<DialogueEntry> = Vec::new();
    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();

    loop {
        // Get game state (type-safe!)
        let game = client.get_game().await?;

        // Fetch explore stats and dialogue (best-effort).
        if let Ok(stats) = client.get_explore_stats().await {
            explore_stats = stats;
        }
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

        // Build status text.
        let status_text = if let Some(ref error) = client.last_error {
            format!(
                "ERROR: {}  •  Arrow keys + Enter | R: Restart | Q: Quit",
                error
            )
        } else if game.is_over() {
            format!("{}  •  R: Restart | Q: Quit", game.status_string())
        } else if let Some(player) = game.to_move() {
            let p = if player == Player::X { "X" } else { "O" };
            format!(
                "Player {} to move  •  Arrow keys + Enter | R: Restart | Q: Quit",
                p
            )
        } else {
            "Waiting…  •  Arrow keys + Enter | R: Restart | Q: Quit".to_string()
        };

        let status_color = if client.last_error.is_some() {
            Color::Red
        } else if game.is_over() {
            Color::Green
        } else {
            Color::Yellow
        };

        // Render UI
        let graph = GraphState::new(true, &ttt_nodes, &ttt_edges, active, &explore_stats);
        let frame_data = FrameData {
            game: &game,
            cursor,
            event_log: &event_log,
            dialogue: &dialogue,
            graph,
            status_text: &status_text,
            status_color,
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
            let agent = standalone::spawn_agent(port, agent_config_path).await?;
            (client, standalone::ProcessGuards::new(server, agent))
        }
        FirstPlayer::Agent => {
            // Spawn agent first → agent plays as X (moves first), human gets O.
            info!("Spawning agent first (agent goes first)");
            let agent = standalone::spawn_agent(port, agent_config_path).await?;
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
    let mut explore_stats = ExploreStats::default();
    let mut dialogue: Vec<DialogueEntry> = Vec::new();

    loop {
        let game = client.get_game().await?;

        // Fetch explore stats and dialogue (best-effort, ignore errors).
        if let Ok(stats) = client.get_explore_stats().await {
            explore_stats = stats;
        }
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
                &explore_stats,
            )
            .await;
        }

        // Render in-progress game.
        {
            use crate::games::tictactoe::Player as TttPlayer;
            let ttt_nodes = tictactoe_nodes();
            let ttt_edges = tictactoe_edges();
            let active = tictactoe_active(&game);
            let status_text = if let Some(ref error) = client.last_error {
                format!("ERROR: {}  •  Arrow keys + Enter | Q: Quit", error)
            } else if let Some(player) = game.to_move() {
                let p = if player == TttPlayer::X { "X" } else { "O" };
                format!("Player {} to move  •  Arrow keys + Enter | Q: Quit", p)
            } else {
                "Waiting…  •  Arrow keys + Enter | Q: Quit".to_string()
            };
            let status_color = if client.last_error.is_some() {
                Color::Red
            } else {
                Color::Yellow
            };
            let graph = GraphState::new(
                show_typestate_graph,
                &ttt_nodes,
                &ttt_edges,
                active,
                &explore_stats,
            );
            let frame_data = FrameData {
                game: &game,
                cursor,
                event_log: &event_log,
                dialogue: &dialogue,
                graph,
                status_text: &status_text,
                status_color,
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
    explore_stats: &ExploreStats,
) -> Result<AnyGame>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crossterm::event::KeyEventKind;
    use tokio::time::{Duration, sleep};

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let active = tictactoe_active(game);

    let status_text = format!(
        "{}  •  Press any key to return to lobby",
        game.status_string()
    );

    let graph = GraphState::new(
        show_typestate_graph,
        &ttt_nodes,
        &ttt_edges,
        active,
        explore_stats,
    );
    let frame_data = FrameData {
        game,
        cursor,
        event_log,
        dialogue,
        graph,
        status_text: &status_text,
        status_color: Color::Green,
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

/// Builds a WCAG-styled [`TextJson`] of the board for use in a TuiNode tree.
#[instrument(skip_all)]
fn build_board_text(
    board: &crate::games::tictactoe::Board,
    cursor: Position,
    pal: &palette::GamePalette,
) -> elicit_ratatui::TextJson {
    use crate::games::tictactoe::{Player, Square};
    use elicit_ratatui::{LineJson, SpanJson, StyleJson, TextJson};

    let positions = [
        [Position::TopLeft, Position::TopCenter, Position::TopRight],
        [
            Position::MiddleLeft,
            Position::Center,
            Position::MiddleRight,
        ],
        [
            Position::BottomLeft,
            Position::BottomCenter,
            Position::BottomRight,
        ],
    ];

    let sep_style = StyleJson {
        fg: Some(pal.border.json.clone()),
        bg: None,
        modifiers: vec![],
    };
    let cursor_style = StyleJson {
        fg: Some(pal.highlight.json.clone()),
        bg: None,
        modifiers: vec![elicit_ratatui::ModifierJson::Bold],
    };
    let x_style = StyleJson {
        fg: Some(pal.warning.json.clone()),
        bg: None,
        modifiers: vec![elicit_ratatui::ModifierJson::Bold],
    };
    let o_style = StyleJson {
        fg: Some(pal.host.json.clone()),
        bg: None,
        modifiers: vec![elicit_ratatui::ModifierJson::Bold],
    };
    let empty_style = StyleJson {
        fg: Some(pal.muted.json.clone()),
        bg: None,
        modifiers: vec![],
    };

    let mut lines: Vec<LineJson> = Vec::new();

    for (row_idx, row) in positions.iter().enumerate() {
        let mut spans: Vec<SpanJson> = Vec::new();

        for (col_idx, &pos) in row.iter().enumerate() {
            let square = board.get(pos);
            let is_cursor = pos == cursor;

            let (text, style) = match (square, is_cursor) {
                (_, true) => {
                    let sym = match square {
                        Square::Empty => " ",
                        Square::Occupied(Player::X) => "X",
                        Square::Occupied(Player::O) => "O",
                    };
                    (format!("[{sym}]"), cursor_style.clone())
                }
                (Square::Empty, false) => (" · ".to_string(), empty_style.clone()),
                (Square::Occupied(Player::X), false) => (" X ".to_string(), x_style.clone()),
                (Square::Occupied(Player::O), false) => (" O ".to_string(), o_style.clone()),
            };

            spans.push(SpanJson {
                content: text,
                style: Some(style),
            });

            if col_idx < 2 {
                spans.push(SpanJson {
                    content: "│".to_string(),
                    style: Some(sep_style.clone()),
                });
            }
        }

        lines.push(LineJson {
            spans,
            style: None,
            alignment: Some(elicit_ratatui::AlignmentJson::Center),
        });

        if row_idx < 2 {
            lines.push(LineJson {
                spans: vec![SpanJson {
                    content: "───┼───┼───".to_string(),
                    style: Some(sep_style.clone()),
                }],
                style: None,
                alignment: Some(elicit_ratatui::AlignmentJson::Center),
            });
        }
    }

    TextJson {
        lines,
        style: None,
        alignment: Some(elicit_ratatui::AlignmentJson::Center),
    }
}

/// Builds a WCAG-styled [`TextJson`] of the event log for the story pane.
#[instrument(skip_all)]
fn build_story_text(
    event_log: &[typestate_widget::GameEvent],
    pal: &palette::GamePalette,
) -> elicit_ratatui::TextJson {
    use elicit_ratatui::{LineJson, ModifierJson, SpanJson, StyleJson, TextJson};

    if event_log.is_empty() {
        return TextJson {
            lines: vec![LineJson {
                spans: vec![SpanJson {
                    content: "Waiting for first move…".to_string(),
                    style: Some(StyleJson {
                        fg: Some(pal.muted.json.clone()),
                        bg: None,
                        modifiers: vec![],
                    }),
                }],
                style: None,
                alignment: None,
            }],
            style: None,
            alignment: None,
        };
    }

    // Show up to 40 lines, most-recent last.
    let max_lines = 40usize;
    let lines: Vec<LineJson> = event_log
        .iter()
        .rev()
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .enumerate()
        .map(|(i, ev)| {
            let age = max_lines.saturating_sub(i + 1);
            let modifiers = if age == 0 {
                vec![ModifierJson::Bold]
            } else {
                vec![]
            };
            let style = if age >= 3 {
                // Old events fade to muted grey.
                StyleJson {
                    fg: Some(pal.muted.json.clone()),
                    bg: None,
                    modifiers: vec![],
                }
            } else if age >= 1 {
                // Recent but not current — use body text colour (AA-verified).
                StyleJson {
                    fg: Some(pal.body.json.clone()),
                    bg: None,
                    modifiers,
                }
            } else {
                // Current event — full palette colour + bold.
                StyleJson {
                    fg: Some(ratatui_color_to_json(ev.color, pal)),
                    bg: None,
                    modifiers,
                }
            };
            LineJson {
                spans: vec![SpanJson {
                    content: ev.text.clone(),
                    style: Some(style),
                }],
                style: None,
                alignment: None,
            }
        })
        .collect();

    TextJson {
        lines,
        style: None,
        alignment: None,
    }
}

/// Maps a ratatui [`ratatui::style::Color`] from the event log to a
/// [`elicit_ratatui::ColorJson`] drawn from the verified palette where
/// possible, falling back to a direct RGB mapping otherwise.
#[instrument(skip_all)]
pub(crate) fn ratatui_color_to_json(
    color: ratatui::style::Color,
    pal: &palette::GamePalette,
) -> elicit_ratatui::ColorJson {
    use ratatui::style::Color;
    match color {
        Color::Cyan | Color::LightCyan => pal.highlight.json.clone(),
        Color::Green | Color::LightGreen => pal.success.json.clone(),
        Color::Yellow | Color::LightYellow => pal.warning.json.clone(),
        Color::Red | Color::LightRed => pal.error.json.clone(),
        Color::Blue | Color::LightBlue => pal.host.json.clone(),
        Color::Magenta | Color::LightMagenta => pal.proof.json.clone(),
        Color::White => pal.title.json.clone(),
        Color::DarkGray | Color::Gray => pal.muted.json.clone(),
        Color::Rgb(r, g, b) => elicit_ratatui::ColorJson::Rgb { r, g, b },
        Color::Indexed(i) => elicit_ratatui::ColorJson::Indexed { index: i },
        _ => pal.agent.json.clone(),
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
    /// Agent explore/play tracking stats.
    explore_stats: &'a ExploreStats,
}

impl<'a> GraphState<'a> {
    /// Creates a new graph state bundle.
    fn new(
        show: bool,
        nodes: &'a [NodeDef],
        edges: &'a [EdgeDef],
        active: Option<usize>,
        explore_stats: &'a ExploreStats,
    ) -> Self {
        Self {
            show,
            nodes,
            edges,
            active,
            explore_stats,
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
    /// Cursor position.
    cursor: Position,
    /// Event log for the story pane.
    event_log: &'a [GameEvent],
    /// Server↔agent dialogue.
    dialogue: &'a [crate::session::DialogueEntry],
    /// Typestate graph state.
    graph: GraphState<'a>,
    /// Status bar text.
    status_text: &'a str,
    /// Status bar colour.
    status_color: Color,
}

/// Renders the 4-column tic-tac-toe layout (Board | Game Story | Chat | Typestate).
///
/// Builds a [`TuiNode`] tree for the outer chrome (title, status bar) and
/// static content (board, story), then renders chat and typestate graph
/// custom widgets into layout-allocated areas.  This is the Phase 4
/// declarative rendering: the layout structure lives in a TuiNode tree;
/// custom widgets slot into areas computed by that tree.
#[instrument(skip_all)]
fn render_tictactoe_frame(f: &mut ratatui::Frame, data: &FrameData) {
    use crate::tui::chat_widget::{ChatMessage, ChatWidget, Participant};
    use crate::tui::contracts::{
        NoOverflow, min_typestate_width, render_resize_prompt, verified_draw,
        verify_typestate_readable,
    };
    use elicit_ratatui::{
        BlockJson, BordersJson, ConstraintJson, DirectionJson, ModifierJson, ParagraphText,
        StyleJson, TuiNode, WidgetJson,
    };
    use elicitation::contracts::Established;
    use palette::GamePalette;
    use ratatui::layout::{Constraint, Direction, Layout};

    let pal = GamePalette::new();
    let has_chat = !data.dialogue.is_empty();

    // --- Helper closures for building palette-sourced StyleJson ---
    let style_fg = |c: &palette::VerifiedColor| StyleJson {
        fg: Some(c.json.clone()),
        bg: None,
        modifiers: vec![],
    };
    let style_fg_bold = |c: &palette::VerifiedColor| StyleJson {
        fg: Some(c.json.clone()),
        bg: None,
        modifiers: vec![ModifierJson::Bold],
    };
    let style_border = |c: &palette::VerifiedColor| StyleJson {
        fg: Some(c.json.clone()),
        bg: None,
        modifiers: vec![],
    };

    // --- Board content as rich TextJson ---
    let board_text = build_board_text(data.game.board(), data.cursor, &pal);

    // --- Story lines as TextJson ---
    let story_text = build_story_text(data.event_log, &pal);

    // --- Adaptive column constraints ---
    // The board is a small 3×3 grid — cap it so it doesn't hoard space.
    // The typestate widget needs at least min_typestate_width cols to render
    // node labels un-truncated; giving it Constraint::Min ensures it always
    // gets enough, stealing from chat/story only when the terminal is large.
    let min_ts = if data.graph.show {
        min_typestate_width(data.graph.nodes)
    } else {
        0
    };
    const BOARD_WIDTH: u16 = 22;

    // When the typestate graph is shown it embeds the story log internally —
    // the standalone story column is omitted to avoid duplication.
    let col_constraints: Vec<ConstraintJson> = match (data.graph.show, has_chat) {
        (true, true) => vec![
            // Board (capped) | Chat (fills) | Typestate (min for readability)
            ConstraintJson::Length { value: BOARD_WIDTH },
            ConstraintJson::Min { value: 20 },
            ConstraintJson::Min { value: min_ts },
        ],
        (true, false) => vec![
            // Board (capped) | Typestate (min for readability)
            ConstraintJson::Length { value: BOARD_WIDTH },
            ConstraintJson::Min { value: min_ts },
        ],
        (false, true) => vec![
            // Board (capped) | Story (fills) | Chat (fills)
            ConstraintJson::Length { value: BOARD_WIDTH },
            ConstraintJson::Min { value: 20 },
            ConstraintJson::Min { value: 20 },
        ],
        (false, false) => vec![
            // Board (capped) | Story (fills)
            ConstraintJson::Length { value: BOARD_WIDTH },
            ConstraintJson::Min { value: 0 },
        ],
    };

    // --- Build TuiNode tree: vertical outer shell with horizontal content row ---
    let board_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(board_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: Some("Center".to_string()),
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Board ".to_string()),
                style: None,
                border_style: Some(style_border(&pal.border)),
                padding: None,
            }),
        }),
    };

    let story_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(story_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Game Story ".to_string()),
                style: None,
                border_style: Some(style_border(&pal.border)),
                padding: None,
            }),
        }),
    };

    // Placeholders for chat and typestate — we render custom widgets into
    // the areas produced by the layout after render_node runs the outer shell.
    let clear_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Clear),
    };

    // When the typestate graph is shown, its widget embeds the story log
    // internally — omit the standalone story column to avoid duplication.
    let mut content_children = vec![board_node];
    if !data.graph.show {
        content_children.push(story_node);
    }
    if has_chat {
        content_children.push(clear_node.clone());
    }
    if data.graph.show {
        content_children.push(clear_node);
    }

    let title_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Plain("Strictly Games — Tic Tac Toe".to_string()),
            style: Some(style_fg_bold(&pal.title)),
            wrap: false,
            scroll: None,
            alignment: Some("Center".to_string()),
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: None,
                style: None,
                border_style: Some(style_border(&pal.border)),
                padding: None,
            }),
        }),
    };

    let status_style = if data.status_color == ratatui::style::Color::Red {
        style_fg_bold(&pal.error)
    } else if data.status_color == ratatui::style::Color::Green {
        style_fg(&pal.success)
    } else {
        style_fg(&pal.warning)
    };

    let status_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Plain(data.status_text.to_string()),
            style: Some(status_style),
            wrap: false,
            scroll: None,
            alignment: Some("Center".to_string()),
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: None,
                style: None,
                border_style: Some(style_border(&pal.border)),
                padding: None,
            }),
        }),
    };

    let root = TuiNode::Layout {
        direction: DirectionJson::Vertical,
        constraints: vec![
            ConstraintJson::Length { value: 3 },
            ConstraintJson::Min { value: 0 },
            ConstraintJson::Length { value: 3 },
        ],
        children: vec![
            title_node,
            TuiNode::Layout {
                direction: DirectionJson::Horizontal,
                constraints: col_constraints,
                children: content_children,
                margin: None,
            },
            status_node,
        ],
        margin: None,
    };

    // Render the TuiNode tree — this handles title, board, story, and status.
    let _proof: Established<NoOverflow> = verified_draw(f, f.area(), &root).unwrap_or_else(|e| {
        render_resize_prompt(f, &e);
        Established::assert()
    });

    // Compute the same layout areas to render custom widgets into.
    // Must mirror col_constraints exactly so custom widgets land in the right slots.
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    let col_constraints_ratatui: Vec<Constraint> = match (data.graph.show, has_chat) {
        (true, true) => vec![
            Constraint::Length(BOARD_WIDTH),
            Constraint::Min(20),
            Constraint::Min(min_ts),
        ],
        (true, false) => vec![
            Constraint::Length(BOARD_WIDTH),
            Constraint::Min(min_ts),
        ],
        (false, true) => vec![
            Constraint::Length(BOARD_WIDTH),
            Constraint::Min(20),
            Constraint::Min(20),
        ],
        (false, false) => vec![Constraint::Length(BOARD_WIDTH), Constraint::Min(0)],
    };

    let content_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_constraints_ratatui)
        .split(outer[1]);

    // Chat pane — custom widget rendered into the layout area.
    // Column index: when graph is shown, story column is absent so chat is col 1;
    // when graph is not shown, story is col 1 and chat is col 2.
    if has_chat {
        let chat_col = if data.graph.show { 1 } else { 2 };
        let chat_messages: Vec<ChatMessage> = data
            .dialogue
            .iter()
            .map(|entry| {
                let participant = if entry.role == "Agent" {
                    Participant::Agent("Agent".to_string())
                } else {
                    Participant::Host
                };
                ChatMessage::new(participant, &entry.text)
            })
            .collect();
        // ChatWidget::new returns a ChatWrapped proof token — wrapping is
        // guaranteed by construction, no runtime check needed.
        let (chat, _chat_proof) = ChatWidget::new(&chat_messages);
        f.render_widget(chat, content_areas[chat_col]);
    }

    // Typestate graph — custom widget rendered into the layout area.
    // Column index: board is always 0; story absent when graph shown;
    // chat (if present) precedes typestate.
    if data.graph.show {
        let type_col = if has_chat { 2 } else { 1 };
        if content_areas.len() > type_col {
            let ts_area = content_areas[type_col];
            // Verify the column is wide enough before rendering — fall back to
            // resize prompt on the typestate area if the terminal is too narrow.
            let _ts_proof = verify_typestate_readable(data.graph.nodes, ts_area)
                .unwrap_or_else(|e| {
                    render_resize_prompt(f, &e);
                    Established::assert()
                });
            let widget = TypestateGraphWidget::new(
                data.graph.nodes,
                data.graph.edges,
                data.graph.active,
                data.event_log,
            )
            .with_explore_stats(data.graph.explore_stats);
            f.render_widget(widget, ts_area);
        }
    }
}
