//! Terminal UI for Strictly Games

#![warn(missing_docs)]

mod app;
mod ui;
mod orchestrator;
mod players;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;

use app::App;
use orchestrator::{GameEvent, Orchestrator};
use players::{HumanPlayer, SimpleAI};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting Strictly Games TUI");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channels for communication
    let (key_tx, key_rx) = mpsc::unbounded_channel();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    // Create players
    let player_x = Box::new(HumanPlayer::new("Human", key_rx));
    let player_o = Box::new(SimpleAI::new("AI"));

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
