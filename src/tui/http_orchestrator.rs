//! HTTP-based game orchestration that polls server for state.

use crate::tui::http_client::HttpGameClient;
use super::orchestrator::GameEvent;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, instrument, warn};

/// Orchestrates HTTP-based gameplay by polling server.
pub struct HttpOrchestrator {
    client: HttpGameClient,
    event_tx: mpsc::UnboundedSender<GameEvent>,
    poll_interval_ms: u64,
}

impl HttpOrchestrator {
    /// Creates a new HTTP orchestrator.
    pub fn new(
        client: HttpGameClient,
        event_tx: mpsc::UnboundedSender<GameEvent>,
    ) -> Self {
        Self {
            client,
            event_tx,
            poll_interval_ms: 500,
        }
    }

    /// Runs the polling loop.
    #[instrument(skip(self))]
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting HTTP game orchestration");

        let mut last_board = String::new();

        loop {
            match self.client.get_board().await {
                Ok(state) => {
                    debug!(
                        current_player = %state.current_player,
                        status = %state.status,
                        "Polled server state"
                    );

                    // Check if board changed
                    let current_board = format!("{:?}", state.board);
                    if current_board != last_board {
                        info!("Board state changed");
                        
                        // Send state update to UI
                        self.event_tx.send(GameEvent::StateChanged(
                            self.format_board(&state.board),
                        ))?;
                        
                        last_board = current_board;
                    }

                    // Check if game is over
                    let game_over = state.status != "InProgress";
                    if game_over {
                        info!(winner = ?state.winner, "Game over");
                        self.event_tx.send(GameEvent::GameOver {
                            winner: state.winner,
                        })?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to poll server");
                }
            }

            sleep(Duration::from_millis(self.poll_interval_ms)).await;
        }
    }

    /// Formats board for display.
    fn format_board(&self, board: &[Option<String>]) -> String {
        let mut result = String::new();
        for (i, cell) in board.iter().enumerate() {
            if i % 3 == 0 && i > 0 {
                result.push('\n');
            }
            match cell {
                Some(mark) => result.push_str(mark),
                None => result.push_str(&format!("{}", i + 1)),
            }
            if i % 3 < 2 {
                result.push_str(" | ");
            }
        }
        result
    }
}
