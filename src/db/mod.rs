//! Database persistence layer for user profiles and game statistics.

// ✅ CORRECT: Private module declarations
mod error;
mod models;
mod repository;
mod schema; // Diesel generated schema - internal use only

// ✅ CORRECT: Crate-level exports via pub use
pub use error::DbError;
pub use models::{AggregatedStats, GameOutcome, GameStat, NewGameStat, NewUser, User};
pub use repository::GameRepository;
