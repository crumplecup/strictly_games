//! Terminal UI for Strictly Games

#![warn(missing_docs)]

mod app;
mod mode;
mod ui;
mod orchestrator;
mod http_orchestrator;
mod players;
mod http_client;
mod rest_client;  // Type-safe REST client
mod standalone;
mod input;  // Cursor movement

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, path::PathBuf};
use tracing::{debug, error, info, instrument};

use crate::games::tictactoe::{AnyGame, Position};
use rest_client::RestGameClient;

/// Run the TUI client
#[instrument(skip_all, fields(server_url = ?server_url, port, agent_config = %agent_config.display()))]
pub async fn run(server_url: Option<String>, port: u16, agent_config: PathBuf) -> Result<()> {
    // Setup logging to file to avoid interfering with TUI
    let log_file = std::fs::File::create("strictly_games_tui.log")?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
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
    let client = match RestGameClient::register(
        actual_server_url,
        session_id,
        "Human".to_string(),
    ).await {
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
    mut client: RestGameClient,  // Make mutable to update last_error
) -> Result<()> {
    use tokio::time::{sleep, Duration};
    use crate::games::tictactoe::Player;
    
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
                    Constraint::Length(3),  // Title
                    Constraint::Min(0),     // Board
                    Constraint::Length(3),  // Status
                    Constraint::Length(3),  // Help
                ])
                .split(f.area());
            
            // Title
            let title = Paragraph::new("Strictly Games - Tic Tac Toe (Type-Safe)")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
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
                    format!("Game Over! {} wins! Press 'r' to restart, 'q' to quit", 
                        if winner == Player::X { "X" } else { "O" })
                } else {
                    "Game Over! Draw! Press 'r' to restart, 'q' to quit".to_string()
                }
            } else if let Some(player) = game.to_move() {
                format!("Player {} to move. Use arrow keys + Enter", 
                    if player == Player::X { "X" } else { "O" })
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
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
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
            }
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        
        // Handle input
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
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
        }
        
        sleep(Duration::from_millis(50)).await;
    }
}

/// Renders board with cursor highlighting.
fn render_board_with_cursor(board: &crate::games::tictactoe::Board, cursor: Position) -> String {
    use crate::games::tictactoe::{Player, Square};
    
    let positions = [
        [Position::TopLeft, Position::TopCenter, Position::TopRight],
        [Position::MiddleLeft, Position::Center, Position::MiddleRight],
        [Position::BottomLeft, Position::BottomCenter, Position::BottomRight],
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
