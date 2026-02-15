//! Phase-specific typestate structs for tic-tac-toe.
//!
//! Each phase is its own distinct type with phase-specific fields.
//! This encodes invariants at compile time - a `Finished` game
//! ALWAYS has an outcome, not `Option<Outcome>`.

use super::action::{Move, MoveError};
use super::contracts::{assert_invariants, LegalMove, MoveContract, Contract};
use super::invariants::{AlternatingTurnInvariant, HistoryConsistentInvariant, Invariant, MonotonicBoardInvariant};
use super::phases::Outcome;
use super::{Board, Player, Position, Square};
use tracing::instrument;

// ─────────────────────────────────────────────────────────────
//  Setup Phase
// ─────────────────────────────────────────────────────────────

/// Game in setup phase - ready to start.
///
/// The board is always empty.
/// No history, no outcome.
#[derive(Debug, Clone)]
pub struct GameSetup {
    board: Board,
}

impl GameSetup {
    /// Creates a new game in setup phase.
    #[instrument]
    pub fn new() -> Self {
        Self {
            board: Board::new(),
        }
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Starts the game with the first player (consumes setup, returns in-progress).
    #[instrument(skip(self))]
    pub fn start(self, first_player: Player) -> GameInProgress {
        GameInProgress {
            board: self.board,
            history: Vec::new(),
            to_move: first_player,
        }
    }
}

impl Default for GameSetup {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────
//  InProgress Phase
// ─────────────────────────────────────────────────────────────

/// Game in progress - can accept moves.
///
/// Invariants enforced by type:
/// - to_move alternates
/// - No outcome yet (outcome is in GameFinished)
#[derive(Debug, Clone)]
pub struct GameInProgress {
    pub(super) board: Board,
    pub(super) history: Vec<Move>,
    pub(super) to_move: Player,
}

impl GameInProgress {
    /// Makes a move, consuming self and transitioning to next state.
    ///
    /// Returns either a new InProgress or a Finished state.
    ///
    /// Contract enforcement:
    /// - Preconditions checked always (LegalMove)
    /// - Postconditions checked in debug builds only
    #[instrument(skip(self))]
    pub fn make_move(self, action: Move) -> Result<GameResult, MoveError> {
        // Store state for postcondition checking
        let before = self.clone();
        
        // Precondition: Check contract
        MoveContract::pre(&self, &action)?;
        
        // Apply move
        let mut game = self;
        game.board.set(action.position, Square::Occupied(action.player));
        game.history.push(action);
        
        // Check for winner using rules module
        if let Some(winner) = super::rules::check_winner(&game.board) {
            return Ok(GameResult::Finished(GameFinished {
                board: game.board,
                history: game.history,
                outcome: Outcome::Winner(winner),
            }));
        }
        
        // Check for draw using rules module
        if super::rules::is_full(&game.board) {
            return Ok(GameResult::Finished(GameFinished {
                board: game.board,
                history: game.history,
                outcome: Outcome::Draw,
            }));
        }
        
        // Continue game
        game.to_move = game.to_move.opponent();
        
        // Postcondition: Verify contract in debug builds
        #[cfg(debug_assertions)]
        MoveContract::post(&before, &game)?;
        
        // Legacy invariant assertions
        assert_invariants(&game);
        
        Ok(GameResult::InProgress(game))
    }
    
    /// Returns the current player to move.
    pub fn to_move(&self) -> Player {
        self.to_move
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Returns move history.
    pub fn history(&self) -> &[Move] {
        &self.history
    }
    
    /// Returns valid positions.
    #[instrument(skip(self))]
    pub fn valid_moves(&self) -> Vec<Position> {
        Position::valid_moves(&self.board)
    }
    
    /// Replays moves from initial state.
    #[instrument]
    pub fn replay(moves: &[Move]) -> Result<GameResult, MoveError> {
        let mut game = GameSetup::new().start(Player::X);
        
        for action in moves {
            match game.make_move(*action)? {
                GameResult::InProgress(g) => game = g,
                GameResult::Finished(g) => return Ok(GameResult::Finished(g)),
            }
        }
        
        Ok(GameResult::InProgress(game))
    }
}

// ─────────────────────────────────────────────────────────────
//  Finished Phase
// ─────────────────────────────────────────────────────────────

/// Game finished - outcome determined.
///
/// The outcome is ALWAYS present (not Option).
/// This struct encodes the invariant at the type level.
#[derive(Debug, Clone)]
pub struct GameFinished {
    board: Board,
    history: Vec<Move>,
    outcome: Outcome,  // ✅ NOT Option
}

impl GameFinished {
    /// Returns the outcome.
    ///
    /// Never returns Option - outcome is guaranteed.
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Returns move history.
    pub fn history(&self) -> &[Move] {
        &self.history
    }
    
    /// Restarts the game (consumes finished, returns setup).
    #[instrument(skip(self))]
    pub fn restart(self) -> GameSetup {
        GameSetup::new()
    }
}

// ─────────────────────────────────────────────────────────────
//  Result Type
// ─────────────────────────────────────────────────────────────

/// Result of making a move.
#[derive(Debug)]
pub enum GameResult {
    /// Game continues.
    InProgress(GameInProgress),
    /// Game finished.
    Finished(GameFinished),
}
