//! Serializable game wrapper for typestate phases.

use super::game::{Draw, Game, InProgress, Won};
use super::position::Position;
use super::types::{Board, Player};
use serde::{Deserialize, Serialize};

/// Serializable wrapper for Game<S> in any phase.
///
/// Since typestate phases can't be directly serialized,
/// we use this enum to wrap all possible phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnyGame {
    /// Game in progress.
    InProgress {
        /// The board state.
        board: Board,
        /// Current player to move.
        to_move: Player,
        /// Move history.
        history: Vec<Position>,
    },
    /// Game ended with winner.
    Won {
        /// The board state.
        board: Board,
        /// The winner.
        winner: Player,
        /// Move history.
        history: Vec<Position>,
    },
    /// Game ended in draw.
    Draw {
        /// The board state.
        board: Board,
        /// Move history.
        history: Vec<Position>,
    },
}

impl From<Game<InProgress>> for AnyGame {
    fn from(game: Game<InProgress>) -> Self {
        AnyGame::InProgress {
            board: game.board().clone(),
            to_move: game.to_move(),
            history: game.history().to_vec(),
        }
    }
}

impl From<Game<Won>> for AnyGame {
    fn from(game: Game<Won>) -> Self {
        AnyGame::Won {
            board: game.board().clone(),
            winner: game.winner(),
            history: game.history().to_vec(),
        }
    }
}

impl From<Game<Draw>> for AnyGame {
    fn from(game: Game<Draw>) -> Self {
        AnyGame::Draw {
            board: game.board().clone(),
            history: game.history().to_vec(),
        }
    }
}

impl From<super::game::GameTransition> for AnyGame {
    fn from(transition: super::game::GameTransition) -> Self {
        use super::game::GameTransition;
        match transition {
            GameTransition::InProgress(g) => g.into(),
            GameTransition::Won(g) => g.into(),
            GameTransition::Draw(g) => g.into(),
        }
    }
}

impl AnyGame {
    /// Returns the board for any game phase.
    pub fn board(&self) -> &Board {
        match self {
            AnyGame::InProgress { board, .. } => board,
            AnyGame::Won { board, .. } => board,
            AnyGame::Draw { board, .. } => board,
        }
    }

    /// Returns the move history for any game phase.
    pub fn history(&self) -> &[Position] {
        match self {
            AnyGame::InProgress { history, .. } => history,
            AnyGame::Won { history, .. } => history,
            AnyGame::Draw { history, .. } => history,
        }
    }

    /// Returns a status string for display.
    pub fn status_string(&self) -> String {
        match self {
            AnyGame::InProgress { to_move, .. } => {
                format!("In progress. Player {:?} to move.", to_move)
            }
            AnyGame::Won { winner, .. } => {
                format!("Game over. Player {:?} wins!", winner)
            }
            AnyGame::Draw { .. } => {
                "Game over. Draw!".to_string()
            }
        }
    }

    /// Returns true if the game is over.
    pub fn is_over(&self) -> bool {
        matches!(self, AnyGame::Won { .. } | AnyGame::Draw { .. })
    }

    /// Returns the current player to move, if game is in progress.
    pub fn to_move(&self) -> Option<Player> {
        match self {
            AnyGame::InProgress { to_move, .. } => Some(*to_move),
            _ => None,
        }
    }

    /// Returns the winner, if game is won.
    pub fn winner(&self) -> Option<Player> {
        match self {
            AnyGame::Won { winner, .. } => Some(*winner),
            _ => None,
        }
    }

    /// Attempts to place a mark, consuming and returning a new AnyGame.
    ///
    /// Returns an error if the game is not in progress or the move is invalid.
    pub fn place(self, pos: Position) -> Result<Self, String> {
        match self {
            AnyGame::InProgress { board, to_move, history } => {
                // Reconstruct typed game
                let game = reconstruct_in_progress(board, to_move, history);
                
                // Perform typestate transition
                match game.place(pos) {
                    Ok(transition) => Ok(transition.into()),
                    Err(e) => Err(e.to_string()),
                }
            }
            AnyGame::Won { .. } => Err("Game is already over (won)".to_string()),
            AnyGame::Draw { .. } => Err("Game is already over (draw)".to_string()),
        }
    }
}

/// Helper to reconstruct Game<InProgress> from components.
fn reconstruct_in_progress(board: Board, to_move: Player, history: Vec<Position>) -> Game<InProgress> {
    use super::game::InProgress;
    use std::marker::PhantomData;
    
    Game {
        board,
        to_move,
        winner: None,
        history,
        _state: PhantomData::<InProgress>,
    }
}
