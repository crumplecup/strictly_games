//! Database models and domain types.

use chrono::NaiveDateTime;
use derive_getters::Getters;
use derive_new::new;
use diesel::prelude::*;
use tracing::instrument;

use crate::db::{DbError, schema};

/// User profile database model.
#[derive(Debug, Clone, Queryable, Identifiable, Selectable, Getters)]
#[diesel(table_name = schema::users)]
pub struct User {
    id: i32,
    display_name: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

/// Insertable user model for creating new users.
#[derive(Debug, Clone, Insertable, new)]
#[diesel(table_name = schema::users)]
pub struct NewUser {
    display_name: String,
}

/// Game statistics database model.
#[derive(Debug, Clone, Queryable, Identifiable, Associations, Selectable, Getters)]
#[diesel(table_name = schema::game_stats)]
#[diesel(belongs_to(User))]
pub struct GameStat {
    id: i32,
    user_id: i32,
    opponent_name: String,
    game_type: String,
    outcome: String,
    played_at: NaiveDateTime,
    moves_count: i32,
    session_id: String,
}

impl GameStat {
    /// Parses the stored outcome string into a [`GameOutcome`] enum.
    #[instrument(skip(self), fields(outcome = %self.outcome))]
    pub fn parse_outcome(&self) -> Result<GameOutcome, DbError> {
        GameOutcome::from_db_string(self.outcome())
    }
}

/// Insertable game stat model for recording new game results.
#[derive(Debug, Clone, Insertable, new, Getters)]
#[diesel(table_name = schema::game_stats)]
pub struct NewGameStat {
    user_id: i32,
    opponent_name: String,
    game_type: String,
    outcome: String,
    moves_count: i32,
    session_id: String,
}

/// Game outcome from the user's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameOutcome {
    /// User won the game.
    Win,
    /// User lost the game.
    Loss,
    /// Game ended in a draw.
    Draw,
}

impl GameOutcome {
    /// Converts outcome to the string stored in the database.
    #[instrument]
    pub fn to_db_string(&self) -> &'static str {
        match self {
            Self::Win => "win",
            Self::Loss => "loss",
            Self::Draw => "draw",
        }
    }

    /// Parses outcome from the string stored in the database.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the string is not a valid outcome value.
    #[instrument(skip(s), fields(s = %s))]
    pub fn from_db_string(s: &str) -> Result<Self, DbError> {
        match s {
            "win" => Ok(Self::Win),
            "loss" => Ok(Self::Loss),
            "draw" => Ok(Self::Draw),
            _ => Err(DbError::new(format!("Invalid outcome: '{}'", s))),
        }
    }
}

/// Aggregated statistics for a user.
#[derive(Debug, Clone, Getters)]
pub struct AggregatedStats {
    total_games: i32,
    wins: i32,
    losses: i32,
    draws: i32,
}

impl AggregatedStats {
    /// Creates new aggregated statistics.
    #[instrument]
    pub fn new(total_games: i32, wins: i32, losses: i32, draws: i32) -> Self {
        Self {
            total_games,
            wins,
            losses,
            draws,
        }
    }

    /// Calculates win rate as a percentage (0.0–100.0).
    #[instrument(skip(self))]
    pub fn win_rate(&self) -> f64 {
        if self.total_games == 0 {
            0.0
        } else {
            (self.wins as f64 / self.total_games as f64) * 100.0
        }
    }
}
