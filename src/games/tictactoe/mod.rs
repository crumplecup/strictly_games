//! Tic-tac-toe game implementation.
//!
//! This implementation demonstrates the elicitation framework's three-layer architecture:
//!
//! ## 1. Elicitation (Type-Safe Construction)
//!
//! Position and Player use `#[derive(Elicit)]` for LLM-driven construction:
//! ```ignore
//! let position = Position::elicit(&peer).await?;  // Select paradigm
//! ```
//!
//! ## 2. Contracts (Proof-Carrying Validation)
//!
//! Using elicitation's formally-verified contract system:
//! ```ignore
//! let proof = validate_move(&action, &game)?;  // Establish proof
//! execute_move(&action, &mut game, proof);     // Type-enforced
//! ```
//!
//! ## 3. Typestate (Phase Enforcement)
//!
//! Game phase encoded in types:
//! - `GameSetup` - initial state, can be started
//! - `GameInProgress` - active game, can accept moves
//! - `GameFinished` - terminal state, outcome determined
//!
//! Transitions consume the game:
//! ```ignore
//! let game = GameSetup::new();
//! let game = game.start(Player::X);  // consumes Setup, returns InProgress
//! let result = game.make_move(action)?;  // consumes InProgress
//! ```

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

// Primary API - typestate architecture
pub use action::{Move, MoveError};
pub use phases::{Finished, InProgress, Outcome, Setup};
pub use position::{Position, ValidPositions};
pub use typestate::{GameSetup, GameInProgress, GameFinished, GameResult};
pub use types::{Board, Player, Square};
pub use wrapper::AnyGame;

/// Alias for clarity in session management.
pub type Mark = Player;

/// Compatibility alias for GameSetup
pub type Game = GameSetup;

