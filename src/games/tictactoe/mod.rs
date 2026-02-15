//! Tic-tac-toe game implementation.
//!
//! This implementation demonstrates the elicitation framework's type-safety principles:
//!
//! ## Typestate State Machine
//!
//! The game phase is encoded in the type:
//! - `Game<InProgress>` - can call `.place()`
//! - `Game<Won>` - can call `.winner()`, no `.place()` method exists
//! - `Game<Draw>` - terminal state, no `.place()` method exists
//!
//! ## Type-Level Contracts
//!
//! The type signature IS the contract:
//! ```ignore
//! impl Game<InProgress> {
//!     pub fn place(self, pos: Position) -> Result<GameTransition, PlaceError>
//!     //          ^^^^  ← Consumes InProgress (proves game can continue)
//!     //                    ^^^^^^^^  ← Only valid positions exist
//!     //                                  ^^^^^^^^^^^^^^  ← Explicit transitions
//! }
//! ```
//!
//! Invalid operations are prevented at compile time:
//! - Can't construct invalid positions (Position enum)
//! - Can't call `place()` on terminal states (no method)
//! - Can't skip state transitions (consuming API)

pub mod position;
pub mod types;
pub mod game;
pub mod wrapper;
mod rules;

// Typestate API - primary interface
pub use game::{Draw, Game, GameTransition, InProgress, PlaceError, Won};
pub use position::Position;
pub use types::{Board, GameState, GameStatus, Player, Square};
pub use wrapper::AnyGame;

// Legacy compatibility during migration (for TUI)
pub use rules::Game as LegacyGame;

/// Alias for clarity in session management.
pub type Mark = Player;
