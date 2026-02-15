//! Serializable game wrapper for typestate phases.
//!
//! Supports both old and new typestate implementations during migration.

use super::game::{Draw as OldDraw, Game as OldGame, InProgress as OldInProgress, Won as OldWon};
use super::typestate::{Game as NewGame, GameResult};
use super::phases::{InProgress as NewInProgress, Setup as NewSetup, Finished as NewFinished, Outcome};
use super::action::Move;
use super::position::Position;
use super::types::{Board, Player};
use serde::{Deserialize, Serialize};

/// Serializable wrapper for Game<S> in any phase.
///
/// Since typestate phases can't be directly serialized,
/// we use this enum to wrap all possible phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnyGame {
    /// Game in setup phase (new architecture).
    Setup {
        /// The board state.
        board: Board,
    },
    /// Game in progress.
    InProgress {
        /// The board state.
        board: Board,
        /// Current player to move.
        to_move: Player,
        /// Move history (new architecture uses Move, old uses Position).
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
    /// Game finished (new architecture - unified outcome).
    Finished {
        /// The board state.
        board: Board,
        /// The outcome.
        outcome: Outcome,
        /// Move history.
        history: Vec<Move>,
    },
}

// ─────────────────────────────────────────────────────────────
//  Old typestate conversions (game.rs)
// ─────────────────────────────────────────────────────────────

impl From<OldGame<OldInProgress>> for AnyGame {
    fn from(game: OldGame<OldInProgress>) -> Self {
        AnyGame::InProgress {
            board: game.board().clone(),
            to_move: game.to_move(),
            history: game.history().to_vec(),
        }
    }
}

impl From<OldGame<OldWon>> for AnyGame {
    fn from(game: OldGame<OldWon>) -> Self {
        AnyGame::Won {
            board: game.board().clone(),
            winner: game.winner(),
            history: game.history().to_vec(),
        }
    }
}

impl From<OldGame<OldDraw>> for AnyGame {
    fn from(game: OldGame<OldDraw>) -> Self {
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

// ─────────────────────────────────────────────────────────────
//  New typestate conversions (typestate.rs)
// ─────────────────────────────────────────────────────────────

impl From<NewGame<NewSetup>> for AnyGame {
    fn from(game: NewGame<NewSetup>) -> Self {
        AnyGame::Setup {
            board: game.board().clone(),
        }
    }
}

impl From<NewGame<NewInProgress>> for AnyGame {
    fn from(game: NewGame<NewInProgress>) -> Self {
        AnyGame::InProgress {
            board: game.board().clone(),
            to_move: game.to_move(),
            history: game.history().iter().map(|m| m.position).collect(),
        }
    }
}

impl From<NewGame<NewFinished>> for AnyGame {
    fn from(game: NewGame<NewFinished>) -> Self {
        AnyGame::Finished {
            board: game.board().clone(),
            outcome: *game.outcome(),
            history: game.history().to_vec(),
        }
    }
}

impl From<GameResult> for AnyGame {
    fn from(result: GameResult) -> Self {
        match result {
            GameResult::InProgress(g) => g.into(),
            GameResult::Finished(g) => g.into(),
        }
    }
}

impl AnyGame {
    /// Returns the board for any game phase.
    pub fn board(&self) -> &Board {
        match self {
            AnyGame::Setup { board } => board,
            AnyGame::InProgress { board, .. } => board,
            AnyGame::Won { board, .. } => board,
            AnyGame::Draw { board, .. } => board,
            AnyGame::Finished { board, .. } => board,
        }
    }

    /// Returns the move history for any game phase (as positions).
    pub fn history(&self) -> Vec<Position> {
        match self {
            AnyGame::Setup { .. } => vec![],
            AnyGame::InProgress { history, .. } => history.clone(),
            AnyGame::Won { history, .. } => history.clone(),
            AnyGame::Draw { history, .. } => history.clone(),
            AnyGame::Finished { history, .. } => history.iter().map(|m| m.position).collect(),
        }
    }

    /// Returns a status string for display.
    pub fn status_string(&self) -> String {
        match self {
            AnyGame::Setup { .. } => "Ready to start".to_string(),
            AnyGame::InProgress { to_move, .. } => {
                format!("In progress. Player {:?} to move.", to_move)
            }
            AnyGame::Won { winner, .. } => {
                format!("Game over. Player {:?} wins!", winner)
            }
            AnyGame::Draw { .. } => {
                "Game over. Draw!".to_string()
            }
            AnyGame::Finished { outcome, .. } => {
                match outcome {
                    Outcome::Winner(player) => format!("Game over. Player {:?} wins!", player),
                    Outcome::Draw => "Game over. Draw!".to_string(),
                }
            }
        }
    }

    /// Returns true if the game is over.
    pub fn is_over(&self) -> bool {
        matches!(self, AnyGame::Won { .. } | AnyGame::Draw { .. } | AnyGame::Finished { .. })
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
            AnyGame::Finished { outcome: Outcome::Winner(player), .. } => Some(*player),
            _ => None,
        }
    }

    /// Attempts to place a mark using old typestate, consuming and returning a new AnyGame.
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
            AnyGame::Setup { .. } => Err("Game hasn't started yet".to_string()),
            AnyGame::Won { .. } => Err("Game is already over (won)".to_string()),
            AnyGame::Draw { .. } => Err("Game is already over (draw)".to_string()),
            AnyGame::Finished { .. } => Err("Game is already over".to_string()),
        }
    }
    
    /// Makes a move using a Move action (new architecture).
    ///
    /// This constructs the Move action and validates it, demonstrating
    /// first-class action modeling.
    pub fn make_move_action(self, action: Move) -> Result<Self, String> {
        // Validate player turn before consuming self
        if let Some(current_player) = self.to_move() {
            if action.player() != current_player {
                return Err(format!("Wrong player: expected {:?}, got {:?}", current_player, action.player()));
            }
        }
        
        // Delegate to place() (wrapper uses old typestate internally for now)
        self.place(action.position())
    }
}

/// Helper to reconstruct Game<InProgress> from components (old typestate).
fn reconstruct_in_progress(board: Board, to_move: Player, history: Vec<Position>) -> OldGame<OldInProgress> {
    use std::marker::PhantomData;
    
    OldGame {
        board,
        to_move,
        winner: None,
        history,
        _state: PhantomData::<OldInProgress>,
    }
}
