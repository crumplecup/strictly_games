use anyhow::Result;
use async_trait::async_trait;
use crate::games::tictactoe::{AnyGame, Position};

pub mod agent;
pub mod human;
pub mod simple_ai;
pub mod http;
pub mod http_human;


/// Trait for players (human or AI).
#[async_trait]
pub trait Player: Send {
    /// Player's name for display.
    fn name(&self) -> &str;
    
    /// Gets the next move from this player.
    async fn get_move(&mut self, game: &AnyGame) -> Result<Position>;
}
