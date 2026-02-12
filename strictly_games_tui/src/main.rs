//! Terminal UI for Strictly Games

#![warn(missing_docs)]

mod app;
mod mode;
mod ui;
mod orchestrator;
mod players;

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
use tracing::info;
use tracing_subscriber::EnvFilter;

use app::App;
use orchestrator::{GameEvent, Orchestrator};
use players::{AgentPlayer, HumanPlayer, SimpleAI};

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
            // Create channel for agent moves
            let (agent_move_tx, agent_move_rx) = mpsc::unbounded_channel();
            
            info!("Agent mode - expecting external MCP server with move channel");
            info!("Note: MCP server must be started separately and connected to agent");
            
            // Agent player waits for moves via channel (no peer for sampling)
            // The external MCP server will send moves through a shared channel
            // TODO: Need to pass agent_move_tx to external server somehow
            
            let player_x = Box::new(HumanPlayer::new("Human", key_rx));
            let player_o = Box::new(AgentPlayer::new("Agent", agent_move_rx, None));
            (player_x, player_o)
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
