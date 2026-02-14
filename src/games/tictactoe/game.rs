//! Typestate-based game engine for tic-tac-toe.
//!
//! This module implements a typestate state machine where the game phase
//! is encoded in the type parameter, making invalid operations impossible.

use super::position::Position;
use super::types::{Board, Player, Square};
use std::marker::PhantomData;
use tracing::instrument;

/// Typestate marker: Game is in progress.
#[derive(Debug, Clone, Copy)]
pub struct InProgress;

/// Typestate marker: Game ended in a win.
#[derive(Debug, Clone, Copy)]
pub struct Won;

/// Typestate marker: Game ended in a draw.
#[derive(Debug, Clone, Copy)]
pub struct Draw;

/// Game state with typestate phase encoding.
///
/// The type parameter `S` encodes the game phase:
/// - `Game<InProgress>` - game is ongoing, moves can be made
/// - `Game<Won>` - game ended with a winner
/// - `Game<Draw>` - game ended in a draw
///
/// Invalid operations are prevented at compile time:
/// - `Game<Won>` has no `place()` method
/// - `Game<InProgress>` has no `winner()` method
#[derive(Debug, Clone)]
pub struct Game<S> {
    pub(crate) board: Board,
    pub(crate) to_move: Player,
    pub(crate) winner: Option<Player>,
    pub(crate) history: Vec<Position>,
    pub(crate) _state: PhantomData<S>,
}

/// Result of placing a mark - explicit state transition.
#[derive(Debug)]
pub enum GameTransition {
    /// Game continues with next player.
    InProgress(Game<InProgress>),
    /// Game ended with a winner.
    Won(Game<Won>),
    /// Game ended in a draw.
    Draw(Game<Draw>),
}

/// Errors that can occur when placing a mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceError {
    /// Square is already occupied.
    SquareOccupied,
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceError::SquareOccupied => write!(f, "Square is already occupied"),
        }
    }
}

impl std::error::Error for PlaceError {}

// ─────────────────────────────────────────────────────────────
//  Constructor - always starts InProgress
// ─────────────────────────────────────────────────────────────

impl Game<InProgress> {
    /// Creates a new game in progress.
    #[instrument]
    pub fn new() -> Self {
        Self {
            board: Board::new(),
            to_move: Player::X,
            winner: None,
            history: Vec::new(),
            _state: PhantomData,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Only InProgress can accept moves (consuming transition)
// ─────────────────────────────────────────────────────────────

impl Game<InProgress> {
    /// Places a mark at the given position, consuming the game and returning a transition.
    ///
    /// This method consumes `self` and returns the next state, which may be:
    /// - `InProgress` if the game continues
    /// - `Won` if this move wins the game
    /// - `Draw` if the board is full with no winner
    ///
    /// # Errors
    ///
    /// Returns `PlaceError::SquareOccupied` if the position is already occupied.
    #[instrument(skip(self), fields(position = ?pos, player = ?self.to_move))]
    pub fn place(mut self, pos: Position) -> Result<GameTransition, PlaceError> {
        // Validate square is empty
        if !self.board.is_empty(pos) {
            return Err(PlaceError::SquareOccupied);
        }

        // Place the mark
        self.board.set(pos, Square::Occupied(self.to_move));
        self.history.push(pos);

        // Check for win
        if let Some(winner) = self.board.winner() {
            return Ok(GameTransition::Won(Game {
                board: self.board,
                to_move: self.to_move,
                winner: Some(winner),
                history: self.history,
                _state: PhantomData::<Won>,
            }));
        }

        // Check for draw
        if self.board.is_full() {
            return Ok(GameTransition::Draw(Game {
                board: self.board,
                to_move: self.to_move,
                winner: None,
                history: self.history,
                _state: PhantomData::<Draw>,
            }));
        }

        // Game continues
        Ok(GameTransition::InProgress(Game {
            board: self.board,
            to_move: self.to_move.opponent(),
            winner: None,
            history: self.history,
            _state: PhantomData::<InProgress>,
        }))
    }

    /// Returns the current player to move.
    pub fn to_move(&self) -> Player {
        self.to_move
    }
}

// ─────────────────────────────────────────────────────────────
//  Common methods available on all phases
// ─────────────────────────────────────────────────────────────

impl<S> Game<S> {
    /// Returns a reference to the board.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// Returns the move history.
    pub fn history(&self) -> &[Position] {
        &self.history
    }
}

// ─────────────────────────────────────────────────────────────
//  Won state - has winner() method
// ─────────────────────────────────────────────────────────────

impl Game<Won> {
    /// Returns the winner of the game.
    ///
    /// This method only exists on `Game<Won>`, providing compile-time
    /// guarantee that a winner exists.
    pub fn winner(&self) -> Player {
        self.winner.expect("Won game must have winner")
    }
}

// ─────────────────────────────────────────────────────────────
//  Draw state - no special methods
// ─────────────────────────────────────────────────────────────

impl Game<Draw> {
    // Draw has no special methods - just board access
}

// ─────────────────────────────────────────────────────────────
//  Board helper methods
// ─────────────────────────────────────────────────────────────

impl Board {
    /// Checks if the board is full.
    pub fn is_full(&self) -> bool {
        self.squares().iter().all(|s| *s != Square::Empty)
    }

    /// Checks for a winner on the board.
    pub fn winner(&self) -> Option<Player> {
        const LINES: [[Position; 3]; 8] = [
            // Rows
            [Position::TopLeft, Position::TopCenter, Position::TopRight],
            [Position::MiddleLeft, Position::Center, Position::MiddleRight],
            [Position::BottomLeft, Position::BottomCenter, Position::BottomRight],
            // Columns
            [Position::TopLeft, Position::MiddleLeft, Position::BottomLeft],
            [Position::TopCenter, Position::Center, Position::BottomCenter],
            [Position::TopRight, Position::MiddleRight, Position::BottomRight],
            // Diagonals
            [Position::TopLeft, Position::Center, Position::BottomRight],
            [Position::TopRight, Position::Center, Position::BottomLeft],
        ];

        for [a, b, c] in LINES {
            let occ = self.get(a);

            if occ != Square::Empty && occ == self.get(b) && occ == self.get(c) {
                return match occ {
                    Square::Occupied(p) => Some(p),
                    Square::Empty => None,
                };
            }
        }

        None
    }
}
