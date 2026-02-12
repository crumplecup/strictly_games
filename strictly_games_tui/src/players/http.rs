//! HTTP-based opponent that polls server for game state.

use super::Player;
use crate::http_client::{BoardState, HttpGameClient};
use anyhow::Result;
use crossterm::event::KeyCode;
use strictly_games::games::tictactoe::Game;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, instrument, warn};

/// Opponent player that connects via HTTP to game server.
/// Polls for state changes and sends human player's moves.
pub struct HttpOpponent {
    name: String,
    client: HttpGameClient,
    input_rx: mpsc::UnboundedReceiver<KeyCode>,
    poll_interval_ms: u64,
    last_state: Option<BoardState>,
}

impl HttpOpponent {
    /// Creates a new HTTP opponent.
    pub fn new(
        name: String,
        client: HttpGameClient,
        input_rx: mpsc::UnboundedReceiver<KeyCode>,
    ) -> Self {
        info!(name = %name, "Creating HTTP opponent");
        Self {
            name,
            client,
            input_rx,
            poll_interval_ms: 500,
            last_state: None,
        }
    }
}

#[async_trait::async_trait]
impl Player for HttpOpponent {
    #[instrument(skip(self, _game))]
    async fn get_move(&mut self, _game: &Game) -> Result<usize> {
        info!("Waiting for opponent's move from server");

        // Poll server until it's their turn (we detect when current_player changes)
        loop {
            match self.client.get_board().await {
                Ok(state) => {
                    debug!(
                        current_player = %state.current_player,
                        status = %state.status,
                        "Polled server state"
                    );

                    // Check if state changed
                    let state_changed = self.last_state.as_ref()
                        .map(|last| last.current_player != state.current_player)
                        .unwrap_or(true);

                    self.last_state = Some(state.clone());

                    if state_changed {
                        info!(current_player = %state.current_player, "Turn changed, opponent moved");
                        // Return dummy value - orchestrator will update from actual game state
                        return Ok(0);
                    }

                    // Check if game is over
                    if state.status.contains("Won") || state.status.contains("Draw") {
                        warn!(status = %state.status, "Game ended");
                        return Err(anyhow::anyhow!("Game ended: {}", state.status));
                    }

                    debug!("No change detected, continuing to poll");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to poll server, retrying");
                }
            }

            // Check for keyboard input (human player making move)
            if let Ok(key) = self.input_rx.try_recv() {
                if let KeyCode::Char(c) = key {
                    if let Some(digit) = c.to_digit(10) {
                        let pos = digit as usize;
                        if pos >= 1 && pos <= 9 {
                            let position = pos - 1;
                            info!(position, "Human player wants to make move");
                            
                            // Send move to server
                            if let Err(e) = self.client.make_move(position).await {
                                warn!(error = %e, position, "Failed to send move to server");
                                return Err(e);
                            }
                            
                            info!(position, "Move sent to server successfully");
                            // Poll immediately to get updated state
                            continue;
                        }
                    }
                }
            }

            sleep(Duration::from_millis(self.poll_interval_ms)).await;
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}
