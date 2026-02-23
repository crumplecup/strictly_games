//! Serializable game wrapper for typestate phases.

use super::action::Move;
use super::phases::Outcome;
use super::position::Position;
use super::types::{Board, Player};
use super::typestate::{GameFinished, GameInProgress, GameResult, GameSetup};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

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
//  New typestate conversions (typestate.rs)
// ─────────────────────────────────────────────────────────────

impl From<GameSetup> for AnyGame {
    fn from(game: GameSetup) -> Self {
        AnyGame::Setup {
            board: game.board().clone(),
        }
    }
}

impl From<GameInProgress> for AnyGame {
    fn from(game: GameInProgress) -> Self {
        AnyGame::InProgress {
            board: game.board().clone(),
            to_move: game.to_move(),
            history: game.history().iter().map(|m| m.position).collect(),
        }
    }
}

impl From<GameFinished> for AnyGame {
    fn from(game: GameFinished) -> Self {
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
            AnyGame::Draw { .. } => "Game over. Draw!".to_string(),
            AnyGame::Finished { outcome, .. } => match outcome {
                Outcome::Winner(player) => format!("Game over. Player {:?} wins!", player),
                Outcome::Draw => "Game over. Draw!".to_string(),
            },
        }
    }

    /// Returns true if the game is over.
    pub fn is_over(&self) -> bool {
        matches!(
            self,
            AnyGame::Won { .. } | AnyGame::Draw { .. } | AnyGame::Finished { .. }
        )
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
            AnyGame::Finished {
                outcome: Outcome::Winner(player),
                ..
            } => Some(*player),
            _ => None,
        }
    }

    /// Makes a move using a Move action (new architecture).
    ///
    /// This uses the NEW typestate architecture with contract validation.
    #[instrument(skip(self))]
    pub fn make_move_action(self, action: Move) -> Result<Self, String> {
        match self {
            AnyGame::InProgress {
                board: _,
                to_move: _,
                history,
            } => {
                // Reconstruct move history (position history → Move actions)
                let mut current_player = Player::X;
                let mut moves: Vec<Move> = history
                    .iter()
                    .map(|&pos| {
                        let mov = Move::new(current_player, pos);
                        current_player = current_player.opponent();
                        mov
                    })
                    .collect();

                // Add the new move
                moves.push(action);

                debug!(
                    move_count = moves.len(),
                    "Replaying moves with contract validation"
                );

                // Replay all moves to reconstruct game state with contract validation
                match GameInProgress::replay(&moves) {
                    Ok(result) => {
                        debug!("Move validated via NEW typestate with contracts");
                        Ok(result.into())
                    }
                    Err(e) => {
                        warn!(error = %e, "Contract validation failed");
                        Err(e.to_string())
                    }
                }
            }
            AnyGame::Setup { .. } => Err("Game hasn't started yet".to_string()),
            AnyGame::Won { .. } => Err("Game is already over (won)".to_string()),
            AnyGame::Draw { .. } => Err("Game is already over (draw)".to_string()),
            AnyGame::Finished { .. } => Err("Game is already over".to_string()),
        }
    }
}
