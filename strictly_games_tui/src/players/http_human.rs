//! HTTP-based human player that sends moves to server.

use super::Player;
use crate::http_client::HttpGameClient;
use anyhow::Result;
use crossterm::event::KeyCode;
use strictly_games::games::tictactoe::Game;
use tokio::sync::mpsc;
use tracing::{info, instrument, warn};

/// Human player that reads keyboard and sends moves via HTTP.
pub struct HttpHumanPlayer {
    name: String,
    client: HttpGameClient,
    input_rx: mpsc::UnboundedReceiver<KeyCode>,
}

impl HttpHumanPlayer {
    /// Creates a new HTTP human player.
    pub fn new(
        name: String,
        client: HttpGameClient,
        input_rx: mpsc::UnboundedReceiver<KeyCode>,
    ) -> Self {
        info!(name = %name, "Creating HTTP human player");
        Self {
            name,
            client,
            input_rx,
        }
    }
}

#[async_trait::async_trait]
impl Player for HttpHumanPlayer {
    #[instrument(skip(self, _game))]
    async fn get_move(&mut self, _game: &Game) -> Result<usize> {
        info!("Waiting for human keyboard input");

        // Wait for keyboard input
        while let Some(key) = self.input_rx.recv().await {
            if let KeyCode::Char(c) = key {
                if let Some(digit) = c.to_digit(10) {
                    let pos = digit as usize;
                    if pos >= 1 && pos <= 9 {
                        let position = pos - 1;
                        
                        // Send move to server
                        match self.client.make_move(position).await {
                            Ok(()) => {
                                info!(position, "Move sent successfully");
                                return Ok(position);
                            }
                            Err(e) => {
                                warn!(error = %e, position, "Failed to send move");
                                // Continue waiting for next input
                            }
                        }
                    }
                }
            }
        }

        anyhow::bail!("Input channel closed")
    }

    fn name(&self) -> &str {
        &self.name
    }
}
