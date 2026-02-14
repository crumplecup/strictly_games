//! Terminal UI for Strictly Games

#![warn(missing_docs)]

mod app;
mod mode;
mod ui;
mod orchestrator;
mod http_orchestrator;
mod players;
mod http_client;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mode::GameMode;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument};

use app::App;
use crate::games::tictactoe::{Game, Position};
use orchestrator::{GameEvent, Orchestrator};
use players::{HumanPlayer, SimpleAI};
use http_client::HttpGameClient;

/// Run the TUI client
#[instrument(skip_all, fields(server_url = %server_url))]
pub async fn run_tui(server_url: String) -> Result<()> {
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for communication
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    
    let session_id = "tui_session".to_string();
    
    info!(server_url = %server_url, session_id = %session_id, "Connecting to HTTP game server");
    
    // Register as human player
    let client = match HttpGameClient::register(
        server_url,
        session_id,
        "Human".to_string(),
    ).await {
        Ok(c) => {
            info!("Successfully registered with server");
            c
        }
        Err(e) => {
            error!(error = %e, "Failed to register with server");
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            return Err(e);
        }
    };
    
    info!("Registered with server, starting HTTP game loop");
    
    // Run HTTP thin client loop
    let res = run_http_game(&mut terminal, client, &mut event_rx).await;
    
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

/// Thin client HTTP game loop - just display and send moves.
#[instrument(skip_all, fields(session_id = %client.session_id, player_id = %client.player_id))]
async fn run_http_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut client: HttpGameClient,
    _event_rx: &mut mpsc::UnboundedReceiver<GameEvent>,
) -> Result<()> {
    use tokio::time::{sleep, Duration};
    
    info!("Starting HTTP thin client game loop");
    
    loop {
        // Poll server for current state
        debug!("Polling server for board state");
        let state = match client.get_board().await {
            Ok(s) => {
                debug!(
                    current_player = %s.current_player,
                    status = %s.status,
                    "Received board state"
                );
                s
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get board state, retrying");
                sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        
        // Draw current board state with centered layout
        terminal.draw(|f| {
            use ratatui::{
                layout::{Alignment, Constraint, Direction, Layout},
                style::{Color, Modifier, Style},
                widgets::{Block, Borders, Paragraph},
            };
            
            // Split screen into sections: title, board, status, help
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // Title
                    Constraint::Min(0),     // Board (centered)
                    Constraint::Length(3),  // Status
                    Constraint::Length(3),  // Help
                ])
                .split(f.area());
            
            // Title
            let title = Paragraph::new("Strictly Games - Tic Tac Toe (HTTP)")
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);
            
            // Board (centered)
            let board_text = format_http_board(&state.board);
            let board = Paragraph::new(board_text)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Board"));
            f.render_widget(board, chunks[1]);
            
            // Status
            let game_over = state.status != "InProgress";
            let status_text = if game_over {
                format!("Game Over! Winner: {}", state.winner.as_deref().unwrap_or("Draw"))
            } else {
                format!("Current player: {}", state.current_player)
            };
            let status = Paragraph::new(status_text)
                .style(Style::default().fg(Color::Yellow))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("Status"));
            f.render_widget(status, chunks[2]);
            
            // Help
            let help_text = if game_over {
                "Game Over! Press R to Restart | Q to Quit"
            } else {
                "Press 1-9 for moves | Q: Quit"
            };
            let help = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[3]);
        })?;
        
        // Check if game over - wait for user input instead of auto-exiting
        let game_over = state.status != "InProgress";
        if game_over {
            info!("Game over detected, status: {}", state.status);
            
            // Wait for user input (R to restart, Q to quit)
            loop {
                if event::poll(Duration::from_millis(100))? {
                    if let Event::Key(key) = event::read()? {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                info!("User quit after game over");
                                return Ok(());
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                info!("User requested restart");
                                // Start a new game
                                if let Err(e) = client.start_game().await {
                                    tracing::warn!(error = %e, "Failed to start new game");
                                } else {
                                    info!("New game started, re-registering player");
                                    // Re-register to get a fresh player_id
                                    if let Err(e) = client.reregister().await {
                                        tracing::warn!(error = %e, "Failed to re-register");
                                    } else {
                                        info!("Re-registered successfully, ready to play");
                                        break; // Exit game over loop, continue main loop
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                sleep(Duration::from_millis(100)).await;
            }
            continue; // Skip to next iteration to show fresh board
        }
        
        // Check for keyboard input (non-blocking)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        info!("User quit");
                        return Ok(());
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        if let Some(digit) = c.to_digit(10) {
                            let pos = digit as usize;
                            if pos >= 1 && pos <= 9 {
                                let position = Position::from_index(pos - 1)
                                    .expect("Position 1-9 must be valid");
                                tracing::info!(position = ?position, "Sending move to server");
                                
                                if let Err(e) = client.make_move(position).await {
                                    tracing::warn!(error = %e, "Move failed");
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Poll rate
        sleep(Duration::from_millis(200)).await;
    }
}

#[instrument(skip(board))]
fn format_http_board(board: &[Option<String>]) -> String {
    let mut result = String::new();
    for (i, cell) in board.iter().enumerate() {
        if i % 3 == 0 && i > 0 {
            result.push_str("\n-----------\n");
        }
        match cell {
            Some(mark) => result.push_str(&format!(" {} ", mark)),
            None => result.push_str(&format!(" {} ", i + 1)),
        }
        if i % 3 < 2 {
            result.push('|');
        }
    }
    result
}
