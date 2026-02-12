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
use tracing::{debug, info, instrument};
use tracing_subscriber::EnvFilter;

use app::App;
use orchestrator::{GameEvent, Orchestrator};
use http_orchestrator::HttpOrchestrator;
use players::{HumanPlayer, HttpHumanPlayer, HttpOpponent, SimpleAI};
use http_client::HttpGameClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging to file to avoid interfering with TUI
    let log_file = std::fs::File::create("strictly_games_tui.log")?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::sync::Arc::new(log_file))
        .with_ansi(false)
        .init();

    info!("Starting Strictly Games TUI");

    // Parse command-line mode argument
    let mode = parse_mode_arg();
    info!(mode = ?mode, "Selected game mode");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for communication
    let (key_tx, key_rx) = mpsc::unbounded_channel();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Create players based on mode
    let (player_x, player_o): (Box<dyn players::Player>, Box<dyn players::Player>) = match mode {
        GameMode::HumanVsAI => {
            let player_x = Box::new(HumanPlayer::new("Human", key_rx));
            let player_o = Box::new(SimpleAI::new("SimpleAI"));
            (player_x, player_o)
        }
        GameMode::AIVsAI => {
            // AI vs AI demo mode
            info!("AI vs AI demo mode");
            let player_x = Box::new(SimpleAI::new("AI-X"));
            let player_o = Box::new(SimpleAI::new("AI-O"));
            (player_x, player_o)
        }
        GameMode::HumanVsAgent => {
            info!("HTTP mode - thin client connecting to game server");
            
            // Parse server URL and session ID from environment or use defaults
            let server_url = std::env::var("GAME_SERVER_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string());
            let session_id = std::env::var("GAME_SESSION_ID")
                .unwrap_or_else(|_| "test_session".to_string());
            
            info!(server_url = %server_url, session_id = %session_id, "Connecting to HTTP game server");
            
            // Register as human player
            let client = HttpGameClient::register(
                server_url,
                session_id,
                "Human".to_string(),
            ).await?;
            
            info!("Registered with server, starting HTTP game loop");
            
            // Run HTTP thin client loop
            let res = run_http_game(&mut terminal, client, &mut event_rx).await;
            
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            
            if let Err(err) = res {
                eprintln!("Error: {:?}", err);
            }
            
            return Ok(());
        }
    };

    // Create orchestrator
    let mut orchestrator = Orchestrator::new(player_x, player_o, event_tx);

    // Spawn orchestrator in background
    let orchestrator_handle = tokio::spawn(async move {
        if let Err(e) = orchestrator.run().await {
            tracing::error!(error = %e, "Orchestrator error");
        }
    });

    let app = App::new();
    let res = run_app(&mut terminal, app, key_tx, &mut event_rx).await;

    // Clean up orchestrator
    orchestrator_handle.abort();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

/// Thin client HTTP game loop - just display and send moves.
#[instrument(skip_all, fields(session_id = %client.session_id, player_id = %client.player_id))]
async fn run_http_game<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    client: HttpGameClient,
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
        
        // Draw current board state
        terminal.draw(|f| {
            use ratatui::prelude::*;
            use ratatui::widgets::{Block, Borders, Paragraph};
            
            let board_text = format_http_board(&state.board);
            let game_over = state.status != "InProgress";
            let status = if game_over {
                format!("Game Over! Winner: {}", state.winner.as_deref().unwrap_or("Draw"))
            } else {
                format!("Current player: {}", state.current_player)
            };
            
            let text = format!("{}\n\n{}\n\nPress 1-9 to move, 'q' to quit", status, board_text);
            
            let paragraph = Paragraph::new(text)
                .block(Block::default().title("Tic-Tac-Toe (HTTP)").borders(Borders::ALL));
            
            f.render_widget(paragraph, f.size());
        })?;
        
        // Check if game over
        if state.status != "InProgress" {
            info!("Game over detected");
            sleep(Duration::from_secs(3)).await;
            return Ok(());
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
                                let position = pos - 1;
                                info!(position, "Sending move to server");
                                
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

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    key_tx: mpsc::UnboundedSender<KeyCode>,
    event_rx: &mut mpsc::UnboundedReceiver<GameEvent>,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Check for UI events from orchestrator
        if let Ok(event) = event_rx.try_recv() {
            app.handle_event(event);
        }

        // Check for keyboard input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('r') => {
                        // TODO: Need to restart orchestrator
                        app.restart();
                    }
                    code => {
                        // Send all keys to human player
                        let _ = key_tx.send(code);
                    }
                }
            }
        }
    }
}

/// Parses game mode from command-line arguments.
fn parse_mode_arg() -> GameMode {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() > 1 {
        match args[1].as_str() {
            "ai" | "simple" => GameMode::HumanVsAI,
            "agent" | "mcp" => GameMode::HumanVsAgent,
            "demo" | "aivsai" => GameMode::AIVsAI,
            _ => {
                eprintln!("Unknown mode: {}. Using default (ai)", args[1]);
                eprintln!("Valid modes: ai, agent, demo");
                GameMode::default()
            }
        }
    } else {
        // No argument provided, show available modes
        eprintln!("Strictly Games TUI");
        eprintln!("Usage: strictly_games_tui [mode]");
        eprintln!("Modes:");
        eprintln!("  ai     - Play against SimpleAI (default)");
        eprintln!("  agent  - Play against MCP agent (requires server running)");
        eprintln!("  demo   - Watch AI vs AI gameplay");
        eprintln!();
        eprintln!("Starting with default mode: ai");
        GameMode::default()
    }
}
