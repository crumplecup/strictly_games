//! Terminal UI for Strictly Games

#![warn(missing_docs)]

mod input; // Cursor movement
mod rest_client; // Type-safe REST client
mod standalone;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, path::PathBuf};
use tracing::{error, info, instrument};

use crate::{
    AgentLibrary, AnyGame, GameRepository, LobbyController, ProfileService, TicTacToePlayer,
};

use crate::games::tictactoe::Position;
use rest_client::RestGameClient;

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

    // Determine mode: standalone or remote
    let (actual_server_url, _guards) = if let Some(url) = server_url {
        // Remote mode: connect to existing server
        info!(server_url = %url, "Connecting to remote server");
        (url, None)
    } else {
        // Standalone mode: spawn server and agent
        info!(port, "Starting standalone mode");
        let guards = standalone::spawn_standalone(port, agent_config).await?;
        let url = format!("http://localhost:{}", port);
        info!(server_url = %url, "Standalone mode initialized");
        (url, Some(guards))
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

    loop {
        // Get game state (type-safe!)
        let game = client.get_game().await?;

        // Render UI
        terminal.draw(|f| {
            use ratatui::{
                layout::{Alignment, Constraint, Direction, Layout},
                style::{Color, Modifier, Style},
                widgets::{Block, Borders, Paragraph},
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Min(0),    // Board
                    Constraint::Length(3), // Status
                    Constraint::Length(3), // Help
                ])
                .split(f.area());

            // Title
            let title = Paragraph::new("Strictly Games - Tic Tac Toe (Type-Safe)")
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            // Board (with cursor highlighting!)
            let board_lines = render_board_with_cursor(game.board(), cursor);
            let board = Paragraph::new(board_lines)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Board"));
            f.render_widget(board, chunks[1]);

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
            f.render_widget(status, chunks[2]);

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
            f.render_widget(help, chunks[3]);
        })?;

        // Handle game over
        if game.is_over() {
            if event::poll(Duration::from_millis(100))?
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
            }
            sleep(Duration::from_millis(100)).await;
            continue;
        }

        // Handle input
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(()),
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

/// Run the lobby TUI (profile selection, agent selection, game stats).
#[instrument(skip_all, fields(db_path = %db_path, port))]
pub async fn run_lobby(
    db_path: String,
    agents_dir: Option<PathBuf>,
    port: u16,
    agent_config: PathBuf,
) -> Result<()> {
    // Set up logging to file so it doesn't interfere with TUI.
    let log_file = std::fs::File::create("strictly_games_lobby.log")?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::sync::Arc::new(log_file))
        .with_ansi(false)
        .try_init();

    info!(db_path = %db_path, "Starting lobby");

    // Run migrations and create repository.
    {
        use diesel::Connection;
        use diesel::SqliteConnection;
        use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
        const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");
        let mut conn = SqliteConnection::establish(&db_path)?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;
        info!("Database migrations applied");
    }

    let repository = GameRepository::new(db_path)?;
    let profile_service = ProfileService::new(repository);

    // Load agent library from configured dir or default.
    let agent_library = if let Some(dir) = agents_dir {
        AgentLibrary::scan(dir)?
    } else {
        AgentLibrary::scan_default().unwrap_or_else(|_| {
            // Fall back to examples directory gracefully.
            AgentLibrary::scan("examples").unwrap_or_else(|e| {
                // If there are truly no configs, this will be caught in the controller.
                panic!("No agent configs found: {}", e)
            })
        })
    };

    info!(agent_count = agent_library.len(), "Agent library ready");

    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run lobby controller.
    let mut controller = LobbyController::new(profile_service, agent_library, agent_config, port);
    let result = controller.run(&mut terminal).await;

    // Restore terminal.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(ref e) = result {
        error!(error = ?e, "Lobby error");
        eprintln!("Lobby error: {:?}", e);
    }

    result
}

/// Runs a complete game session from the lobby: spawns server + agent, plays
/// one game to completion, and returns the final game state and the human's mark.
///
/// Unlike [`run`], this does not restart the game or exit on 'q'; pressing any
/// key after the game ends returns the outcome to the lobby controller.
#[instrument(skip(terminal), fields(player_name = %player_name, port))]
pub async fn run_game_session<B: ratatui::backend::Backend + std::io::Write>(
    terminal: &mut Terminal<B>,
    agent_config_path: PathBuf,
    player_name: String,
    port: u16,
) -> Result<(AnyGame, TicTacToePlayer)>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    info!("Starting lobby game session");

    // Spawn standalone server + agent subprocess.
    let _guards = standalone::spawn_standalone(port, agent_config_path).await?;
    let server_url = format!("http://localhost:{}", port);

    // Register human player.
    let client =
        RestGameClient::register(server_url, "tui_session".to_string(), player_name).await?;

    let human_mark = client.player_mark;
    info!(mark = ?human_mark, "Human player registered");

    // Play one game to completion.
    let final_game = run_lobby_game(terminal, client).await?;
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
) -> Result<AnyGame>
where
    <B as ratatui::backend::Backend>::Error: Send + Sync + 'static,
{
    use crate::games::tictactoe::Player;
    use crossterm::event::KeyEventKind;
    use tokio::time::{Duration, sleep};

    info!("Starting lobby game loop");

    let mut cursor = Position::Center;

    loop {
        let game = client.get_game().await?;

        // Once game is over, render final state and wait for any keypress.
        if game.is_over() {
            terminal.draw(|f| {
                use ratatui::{
                    layout::{Alignment, Constraint, Direction, Layout},
                    style::{Color, Modifier, Style},
                    widgets::{Block, Borders, Paragraph},
                };

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(3),
                        Constraint::Length(3),
                    ])
                    .split(f.area());

                let title = Paragraph::new("Strictly Games - Tic Tac Toe")
                    .style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(title, chunks[0]);

                let board_lines = render_board_with_cursor(game.board(), cursor);
                let board = Paragraph::new(board_lines)
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title("Board"));
                f.render_widget(board, chunks[1]);

                let status_text = game.status_string();
                let status = Paragraph::new(status_text)
                    .style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title("Result"));
                f.render_widget(status, chunks[2]);

                let help = Paragraph::new("Press any key to return to lobby")
                    .style(Style::default().fg(Color::DarkGray))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(help, chunks[3]);
            })?;

            // Wait for any keypress.
            loop {
                if event::poll(Duration::from_millis(100))?
                    && let Event::Key(key) = event::read()?
                    && key.kind == KeyEventKind::Press
                {
                    return Ok(game);
                }
                sleep(Duration::from_millis(50)).await;
            }
        }

        // Render in-progress game.
        terminal.draw(|f| {
            use ratatui::{
                layout::{Alignment, Constraint, Direction, Layout},
                style::{Color, Modifier, Style},
                widgets::{Block, Borders, Paragraph},
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                    Constraint::Length(3),
                ])
                .split(f.area());

            let title = Paragraph::new("Strictly Games - Tic Tac Toe")
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            let board_lines = render_board_with_cursor(game.board(), cursor);
            let board = Paragraph::new(board_lines)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Board"));
            f.render_widget(board, chunks[1]);

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
            f.render_widget(status, chunks[2]);

            let help_text = if let Some(ref error) = client.last_error {
                format!("ERROR: {}", error)
            } else {
                "Arrow keys: Move | Enter: Place".to_string()
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
            f.render_widget(help, chunks[3]);
        })?;

        // Handle input.
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
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
