//! Typestate-based game state machine for tic-tac-toe.
//!
//! The game progresses through distinct phases, each represented by a type parameter:
//! - `Game<Setup>` - Initial state, no players assigned
//! - `Game<InProgress>` - Active game, moves can be made
//! - `Game<Finished>` - Game over, outcome determined
//!
//! Transitions between phases consume the game and return the next phase,
//! making illegal operations impossible at compile time.

use super::action::{Move, MoveError};
use super::contracts::{assert_invariants, LegalMove};
use super::phases::{Finished, InProgress, Outcome, Setup};
use super::{Board, Player, Square};
use std::marker::PhantomData;
use tracing::instrument;

/// Game state with typestate phase encoding.
///
/// The type parameter `Phase` encodes the current game phase:
/// - `Game<Setup>` - can be started
/// - `Game<InProgress>` - can accept moves
/// - `Game<Finished>` - can be inspected for outcome
///
/// Invalid operations are prevented at compile time.
#[derive(Debug, Clone)]
pub struct Game<Phase> {
    board: Board,
    history: Vec<Move>,
    to_move: Player,
    outcome: Option<Outcome>,
    _phase: PhantomData<Phase>,
}

// ─────────────────────────────────────────────────────────────
//  Setup Phase
// ─────────────────────────────────────────────────────────────

impl Game<Setup> {
    /// Creates a new game in setup phase.
    #[instrument]
    pub fn new() -> Self {
        Self {
            board: Board::new(),
            history: Vec::new(),
            to_move: Player::X,
            outcome: None,
            _phase: PhantomData,
        }
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Starts the game with the first player to move (consumes setup, returns in-progress).
    #[instrument(skip(self))]
    pub fn start(self, first_player: Player) -> Game<InProgress> {
        Game {
            board: self.board,
            history: self.history,
            to_move: first_player,
            outcome: None,
            _phase: PhantomData,
        }
    }
}

impl Default for Game<Setup> {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────
//  InProgress Phase
// ─────────────────────────────────────────────────────────────

impl Game<InProgress> {
    /// Makes a move, consuming the game and transitioning to the next state.
    ///
    /// This method validates the move using contracts, applies it, and returns
    /// either a new `InProgress` state or a `Finished` state with outcome.
    ///
    /// # Errors
    ///
    /// Returns `MoveError` if:
    /// - The square is already occupied
    /// - It's not the player's turn
    #[instrument(skip(self))]
    pub fn make_move(mut self, action: Move) -> Result<GameResult, MoveError> {
        // Contract-based validation
        LegalMove::check(&action, &self)?;
        
        // Apply the move (pure operation)
        self.board.set(action.position, Square::Occupied(action.player));
        self.history.push(action);
        
        // Check for winner
        if let Some(winner) = self.board.winner() {
            return Ok(GameResult::Finished(Game {
                board: self.board,
                history: self.history,
                to_move: self.to_move,
                outcome: Some(Outcome::Winner(winner)),
                _phase: PhantomData,
            }));
        }
        
        // Check for draw
        if self.board.is_full() {
            return Ok(GameResult::Finished(Game {
                board: self.board,
                history: self.history,
                to_move: self.to_move,
                outcome: Some(Outcome::Draw),
                _phase: PhantomData,
            }));
        }
        
        // Game continues with next player
        self.to_move = self.to_move.opponent();
        
        // Assert invariants hold (debug only)
        assert_invariants(&self);
        
        Ok(GameResult::InProgress(self))
    }
    
    /// Returns the current player to move.
    pub fn to_move(&self) -> Player {
        self.to_move
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Returns the move history.
    pub fn history(&self) -> &[Move] {
        &self.history
    }
}

// ─────────────────────────────────────────────────────────────
//  Finished Phase
// ─────────────────────────────────────────────────────────────

impl Game<Finished> {
    /// Returns the outcome of the finished game.
    pub fn outcome(&self) -> &Outcome {
        self.outcome.as_ref().expect("Finished game must have outcome")
    }
    
    /// Returns the board.
    pub fn board(&self) -> &Board {
        &self.board
    }
    
    /// Returns the move history.
    pub fn history(&self) -> &[Move] {
        &self.history
    }
}

// ─────────────────────────────────────────────────────────────
//  Result Type for Move Transitions
// ─────────────────────────────────────────────────────────────

/// Result of making a move: either the game continues or finishes.
#[derive(Debug)]
pub enum GameResult {
    /// Game continues in progress.
    InProgress(Game<InProgress>),
    /// Game has finished with an outcome.
    Finished(Game<Finished>),
}

// ─────────────────────────────────────────────────────────────
//  Replay Capability
// ─────────────────────────────────────────────────────────────

impl Game<InProgress> {
    /// Replays a sequence of moves from the initial state.
    ///
    /// This is useful for:
    /// - Reconstructing game state from history
    /// - Testing move sequences
    /// - Debugging game flow
    ///
    /// # Errors
    ///
    /// Returns the first `MoveError` encountered during replay.
    #[instrument]
    pub fn replay(moves: &[Move]) -> Result<GameResult, MoveError> {
        let mut game = Game::<Setup>::new().start(Player::X);
        
        for action in moves {
            match game.make_move(*action)? {
                GameResult::InProgress(g) => game = g,
                GameResult::Finished(g) => return Ok(GameResult::Finished(g)),
            }
        }
        
        Ok(GameResult::InProgress(game))
    }
}
