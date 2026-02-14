//! Human player that gets input from keyboard.

use super::Player;
use anyhow::Result;
use crossterm::event::KeyCode;
use crate::games::tictactoe::Game;
use tokio::sync::mpsc;

/// Human player using keyboard input.
pub struct HumanPlayer {
    name: String,
    input_rx: mpsc::UnboundedReceiver<KeyCode>,
}

impl HumanPlayer {
    /// Creates a new human player.
    pub fn new(name: impl Into<String>, input_rx: mpsc::UnboundedReceiver<KeyCode>) -> Self {
        Self {
            name: name.into(),
            input_rx,
        }
    }
}

#[async_trait::async_trait]
impl Player for HumanPlayer {
    async fn get_move(&mut self, _game: &Game) -> Result<usize> {
        // Wait for keyboard input
        while let Some(key) = self.input_rx.recv().await {
            if let KeyCode::Char(c) = key {
                if let Some(digit) = c.to_digit(10) {
                    let pos = digit as usize;
                    if pos >= 1 && pos <= 9 {
                        return Ok(pos - 1);
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
