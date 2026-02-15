//! Tic-tac-toe game implementation.
//!
//! This implementation demonstrates the elicitation framework's architectural principles:
//!
//! ## Typestate State Machine
//!
//! The game phase is encoded in the type parameter:
//! - `Game<Setup>` - initial state, can be started
//! - `Game<InProgress>` - active game, can accept moves
//! - `Game<Finished>` - terminal state, outcome determined
//!
//! Transitions consume the game:
//! ```ignore
//! let game = Game::<Setup>::new();
//! let game = game.start(Player::X);  // consumes Setup, returns InProgress
//! let result = game.make_move(action);  // consumes InProgress, returns InProgress or Finished
//! ```
//!
//! ## First-Class Actions
//!
//! Moves are domain events with independent validation:
//! ```ignore
//! let action = Move::new(Player::X, Position::Center);
//! LegalMove::check(&action, &game)?;  // Validate via contracts
//! let result = game.make_move(action)?;  // Apply action
//! ```
//!
//! ## Contract-Driven Validation
//!
//! Rules are declarative contracts, not imperative checks:
//! - `SquareIsEmpty` - precondition for placing a mark
//! - `PlayersTurn` - precondition for move legality
//! - `BoardConsistent` - invariant on board state
//!
//! ## Clean Boundaries
//!
//! Domain contains pure state, no presentation logic:
//! - Game types know nothing about rendering
//! - UI concerns handled separately
//! - Domain is reusable across contexts

// Core domain types
pub mod position;
pub mod types;

// Game rules (pure functions)
pub mod rules;

// Typestate architecture
pub mod phases;
pub mod action;
pub mod contracts;
pub mod typestate;

// Wrapper for session management
pub mod wrapper;

// Primary API - new typestate architecture
pub use action::{Move, MoveError};
pub use phases::{Finished, InProgress, Outcome, Setup};
pub use position::Position;
pub use rules::{check_winner, is_draw, is_full};
pub use typestate::{GameSetup, GameInProgress, GameFinished, GameResult};
pub use types::{Board, Player, Square};
pub use wrapper::AnyGame;

/// Alias for clarity in session management.
pub type Mark = Player;

/// Compatibility alias for Game<Setup>
pub type Game = GameSetup;

