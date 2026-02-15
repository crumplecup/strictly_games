//! First-class action types for tic-tac-toe.
//!
//! Moves are domain events, not side effects. They represent
//! the player's intent and can be validated independently of execution.

use super::{Player, Position};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// A move in tic-tac-toe: a player placing their mark at a position.
///
/// Moves are first-class domain events that can be:
/// - Validated before application
/// - Serialized for replay
/// - Logged for debugging
/// - Reasoned about by contracts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Move {
    /// The player making the move.
    pub player: Player,
    /// The position where the player places their mark.
    pub position: Position,
}

impl Move {
    /// Creates a new move.
    #[instrument]
    pub fn new(player: Player, position: Position) -> Self {
        Self { player, position }
    }
    
    /// Returns the player making this move.
    pub fn player(&self) -> Player {
        self.player
    }
    
    /// Returns the position of this move.
    pub fn position(&self) -> Position {
        self.position
    }
}

impl std::fmt::Display for Move {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} -> {}", self.player, self.position.label())
    }
}

/// Error that can occur when validating or applying a move.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum MoveError {
    /// The square at the position is already occupied.
    #[display("Square {:?} is already occupied", _0)]
    SquareOccupied(Position),
    
    /// The game is already over.
    #[display("Game is already over")]
    GameOver,
    
    /// It's not this player's turn.
    #[display("It's not {:?}'s turn", _0)]
    WrongPlayer(Player),
    
    /// An invariant was violated (postcondition failure).
    #[display("Invariant violation: {}", _0)]
    InvariantViolation(String),
}

impl std::error::Error for MoveError {}
