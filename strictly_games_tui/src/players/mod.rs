//! Player trait and implementations.

mod human;
mod agent;
mod simple_ai;

pub use human::HumanPlayer;
pub use agent::AgentPlayer;
pub use simple_ai::SimpleAI;

use anyhow::Result;
use strictly_games::games::tictactoe::Game;

/// Trait for players that can make moves.
#[async_trait::async_trait]
pub trait Player: Send {
    /// Gets a move from this player.
    /// 
    /// Returns the position (0-8) for the next move.
    async fn get_move(&mut self, game: &Game) -> Result<usize>;
    
    /// Returns the player's display name.
    fn name(&self) -> &str;
}
