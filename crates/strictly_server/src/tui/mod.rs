//! Terminal UI for Strictly Games

#![warn(missing_docs)]

pub mod blackjack;
pub mod chat_widget;
mod input; // Cursor movement
pub mod mcp_communicator;
pub mod observable_communicator;
mod rest_client; // Type-safe REST client
mod standalone;
pub mod tui_communicator;
mod typestate_widget;

pub use blackjack::{BlackjackSessionOutcome, run_blackjack_session, run_multi_blackjack_session};
pub use chat_widget::{ChatMessage, ChatWidget, Participant, chat_channel};
pub use mcp_communicator::LlmElicitCommunicator;

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
use typestate_widget::{
    GameEvent, TypestateGraphWidget, tictactoe_active, tictactoe_edges, tictactoe_nodes,
    tictactoe_phase_name,
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
    use tokio::time::{Duration, sleep};

    info!("Starting type-safe game loop");

    let mut cursor = Position::Center;
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut prev_phase: Option<&'static str> = None;
    let mut prev_move_count: usize = 0;
    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();

    loop {
        // Get game state (type-safe!)
        let game = client.get_game().await?;

        // Track phase transitions and proof events.
        let current_phase = tictactoe_phase_name(&game);
        if Some(current_phase) != prev_phase {
            if let Some(prev) = prev_phase {
                event_log.push(GameEvent::phase_change(prev, current_phase));
            }
            if game.is_over() {
                event_log.push(GameEvent::result(game.status_string()));
            }
            prev_phase = Some(current_phase);
        }
        let move_count = game.history().len();
        if move_count > prev_move_count {
            event_log.push(GameEvent::proof("LegalMove"));
            prev_move_count = move_count;
        }

        let active = tictactoe_active(&game);

        // Render UI
        terminal.draw(|f| {
            use ratatui::{
                layout::{Alignment, Constraint, Direction, Layout},
                style::{Color, Modifier, Style},
                widgets::{Block, Borders, Paragraph},
            };

            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Min(0),    // Content
                    Constraint::Length(3), // Status
                    Constraint::Length(3), // Help
                ])
                .split(f.area());

            let content = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(55), // Board
                    Constraint::Percentage(45), // Typestate graph
                ])
                .split(outer[1]);

            // Title
            let title = Paragraph::new("Strictly Games - Tic Tac Toe (Type-Safe)")
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, outer[0]);

            // Board (with cursor highlighting!)
            let board_lines = render_board_with_cursor(game.board(), cursor);
            let board = Paragraph::new(board_lines)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Board"));
            f.render_widget(board, content[0]);

            // Typestate graph.
            f.render_widget(
                TypestateGraphWidget::new(&ttt_nodes, &ttt_edges, active, &event_log),
                content[1],
            );

            // Status (type-safe!)
            let status_text = if game.is_over() {
                if let Some(winner) = game.winner() {
                    format!(
                        "Game Over! {} wins! Press 'r' to restart, 'q' to quit",
                        if winner == Player::X { "X" } else { "O" }
                    )
                } else {
                    "Game Over! Draw! Press 'r' to restart, 'q' to quit".to_string()
                }
            } else if let Some(player) = game.to_move() {
                format!(
                    "Player {} to move. Use arrow keys + Enter",
                    if player == Player::X { "X" } else { "O" }
                )
            } else {
                "Waiting...".to_string()
            };

            // Color status based on errors
            let status_color = if client.last_error.is_some() {
                Color::Red
            } else {
                Color::Yellow
            };

            let status = Paragraph::new(status_text)
                .style(Style::default().fg(status_color))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(status, outer[2]);

            // Help / Error message
            let help_text = if let Some(ref error) = client.last_error {
                format!("ERROR: {}", error)
            } else {
                "Arrow keys: Move | Enter: Place | Q: Quit | R: Restart".to_string()
            };

            let help_color = if client.last_error.is_some() {
                Color::Red
            } else {
                Color::DarkGray
            };

            let help = Paragraph::new(help_text)
                .style(Style::default().fg(help_color))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, outer[3]);
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
    use tokio::time::{Duration, sleep};

    info!(show_typestate_graph, "Starting lobby game loop");

    let mut cursor = Position::Center;
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut prev_phase: Option<&'static str> = None;
    let mut prev_move_count: usize = 0;

    loop {
        let game = client.get_game().await?;

        // Track phase transitions and proof events.
        let current_phase = tictactoe_phase_name(&game);
        if Some(current_phase) != prev_phase {
            if let Some(prev) = prev_phase {
                event_log.push(GameEvent::phase_change(prev, current_phase));
            }
            if game.is_over() {
                event_log.push(GameEvent::result(game.status_string()));
            }
            prev_phase = Some(current_phase);
        }
        let move_count = game.history().len();
        if move_count > prev_move_count {
            event_log.push(GameEvent::proof("LegalMove"));
            prev_move_count = move_count;
        }

        // Once game is over, render final state and wait for any keypress.
        if game.is_over() {
            return render_game_over_and_wait(
                terminal,
                &game,
                cursor,
                &event_log,
                show_typestate_graph,
            )
            .await;
        }

        // Render in-progress game.
        render_active_game(
            terminal,
            &game,
            &client,
            cursor,
            &event_log,
            show_typestate_graph,
        )?;

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
    show_typestate_graph: bool,
) -> Result<AnyGame>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crossterm::event::KeyEventKind;
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        widgets::{Block, Borders, Paragraph},
    };
    use tokio::time::{Duration, sleep};

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let active = tictactoe_active(game);

    terminal.draw(|f| {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(f.area());

        let board_area = if show_typestate_graph {
            let content = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(outer[1]);
            f.render_widget(
                TypestateGraphWidget::new(&ttt_nodes, &ttt_edges, active, event_log),
                content[1],
            );
            content[0]
        } else {
            outer[1]
        };

        let title = Paragraph::new("Strictly Games - Tic Tac Toe")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, outer[0]);

        let board_lines = render_board_with_cursor(game.board(), cursor);
        let board = Paragraph::new(board_lines)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Board"));
        f.render_widget(board, board_area);

        let status_text = game.status_string();
        let status = Paragraph::new(status_text)
            .style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Result"));
        f.render_widget(status, outer[2]);

        let help = Paragraph::new("Press any key to return to lobby")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, outer[3]);
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

/// Renders the active game state with cursor.
#[instrument(skip_all)]
fn render_active_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    game: &AnyGame,
    client: &RestGameClient,
    cursor: Position,
    event_log: &[GameEvent],
    show_typestate_graph: bool,
) -> Result<()>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crate::games::tictactoe::Player;
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        widgets::{Block, Borders, Paragraph},
    };

    let ttt_nodes = tictactoe_nodes();
    let ttt_edges = tictactoe_edges();
    let active = tictactoe_active(game);

    terminal.draw(|f| {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(f.area());

        let board_area = if show_typestate_graph {
            let content = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(outer[1]);
            f.render_widget(
                TypestateGraphWidget::new(&ttt_nodes, &ttt_edges, active, event_log),
                content[1],
            );
            content[0]
        } else {
            outer[1]
        };

        let title = Paragraph::new("Strictly Games - Tic Tac Toe")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, outer[0]);

        let board_lines = render_board_with_cursor(game.board(), cursor);
        let board = Paragraph::new(board_lines)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Board"));
        f.render_widget(board, board_area);

        let status_text = if let Some(player) = game.to_move() {
            format!(
                "Player {} to move. Use arrow keys + Enter",
                if player == Player::X { "X" } else { "O" }
            )
        } else {
            "Waiting...".to_string()
        };

        let status_color = if client.last_error.is_some() {
            Color::Red
        } else {
            Color::Yellow
        };
        let status = Paragraph::new(status_text)
            .style(Style::default().fg(status_color))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Status"));
        f.render_widget(status, outer[2]);

        let help_text = if let Some(ref error) = client.last_error {
            format!("ERROR: {}", error)
        } else {
            "Arrow keys: Move | Enter: Place | Q: Quit".to_string()
        };
        let help_color = if client.last_error.is_some() {
            Color::Red
        } else {
            Color::DarkGray
        };
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(help_color))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, outer[3]);
    })?;

    Ok(())
}

/// Renders board with cursor highlighting.
fn render_board_with_cursor(board: &crate::games::tictactoe::Board, cursor: Position) -> String {
    use crate::games::tictactoe::{Player, Square};

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

    let mut lines = Vec::new();

    for (row_idx, row) in positions.iter().enumerate() {
        let mut line_spans = Vec::new();

        for (col_idx, &pos) in row.iter().enumerate() {
            let square = board.get(pos);
            let symbol = match square {
                Square::Empty => " ",
                Square::Occupied(Player::X) => "X",
                Square::Occupied(Player::O) => "O",
            };

            let cell = if pos == cursor {
                format!("[{}]", symbol) // Highlight cursor
            } else {
                format!(" {} ", symbol)
            };

            line_spans.push(cell);

            if col_idx < 2 {
                line_spans.push("|".to_string());
            }
        }

        lines.push(line_spans.join(""));

        if row_idx < 2 {
            lines.push("-----------".to_string());
        }
    }

    lines.join("\n")
}
