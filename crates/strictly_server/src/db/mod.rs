//! Database persistence layer for user profiles and game statistics.

mod models;
mod repository;

pub use elicit_db::DbError;
pub use models::{AggregatedStats, GameOutcome, GameStat, NewGameStat, User};
pub use repository::GameRepository;
