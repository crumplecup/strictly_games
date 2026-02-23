//! Database repository for game statistics and user profiles.

use diesel::prelude::*;
use tracing::{debug, info, instrument, warn};

use crate::db::{AggregatedStats, DbError, GameStat, NewGameStat, NewUser, User, schema};

/// Database repository for user and game operations.
#[derive(Debug, Clone)]
pub struct GameRepository {
    db_path: String,
}

impl GameRepository {
    /// Creates a new repository connected to the database at the given path.
    ///
    /// Use `":memory:"` for an in-memory database (useful for tests).
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the path is invalid.
    #[instrument(skip(db_path), fields(db_path = %db_path))]
    pub fn new(db_path: String) -> Result<Self, DbError> {
        info!(path = %db_path, "Creating GameRepository");
        Ok(Self { db_path })
    }

    /// Establishes a database connection.
    #[instrument(skip(self))]
    fn connection(&self) -> Result<SqliteConnection, DbError> {
        debug!(path = %self.db_path, "Establishing connection");
        SqliteConnection::establish(&self.db_path)
            .map_err(|e| DbError::new(format!("Failed to connect to '{}': {}", self.db_path, e)))
    }

    /// Creates a new user profile.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the display name is already taken or a database error occurs.
    #[instrument(skip(self))]
    pub fn create_user(&self, display_name: String) -> Result<User, DbError> {
        debug!(display_name = %display_name, "Creating user");
        let mut conn = self.connection()?;

        let new_user = NewUser::new(display_name);

        let user = diesel::insert_into(schema::users::table)
            .values(&new_user)
            .returning(User::as_returning())
            .get_result(&mut conn)?;

        info!(user_id = user.id(), display_name = %user.display_name(), "User created");
        Ok(user)
    }

    /// Gets a user by display name. Returns `None` if not found.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub fn get_user_by_name(&self, display_name: &str) -> Result<Option<User>, DbError> {
        debug!(display_name = %display_name, "Looking up user by name");
        let mut conn = self.connection()?;

        let user = schema::users::table
            .filter(schema::users::display_name.eq(display_name))
            .first::<User>(&mut conn)
            .optional()?;

        if let Some(ref u) = user {
            debug!(user_id = u.id(), "User found");
        } else {
            debug!("User not found");
        }

        Ok(user)
    }

    /// Lists all user profiles, ordered by creation time.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub fn list_users(&self) -> Result<Vec<User>, DbError> {
        debug!("Listing all users");
        let mut conn = self.connection()?;

        let users = schema::users::table
            .order(schema::users::created_at.asc())
            .load::<User>(&mut conn)?;

        info!(count = users.len(), "Users loaded");
        Ok(users)
    }

    /// Records a completed game result.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self, stat), fields(user_id = stat.user_id(), game_type = %stat.game_type(), outcome = %stat.outcome()))]
    pub fn record_game(&self, stat: NewGameStat) -> Result<GameStat, DbError> {
        debug!("Recording game result");
        let mut conn = self.connection()?;

        let game_stat = diesel::insert_into(schema::game_stats::table)
            .values(&stat)
            .returning(GameStat::as_returning())
            .get_result(&mut conn)?;

        info!(
            stat_id = game_stat.id(),
            user_id = game_stat.user_id(),
            outcome = %game_stat.outcome(),
            "Game result recorded"
        );
        Ok(game_stat)
    }

    /// Gets all game stats for a user, ordered most recent first.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub fn get_user_stats(&self, user_id: i32) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id = %user_id, "Loading user stats");
        let mut conn = self.connection()?;

        let stats = schema::game_stats::table
            .filter(schema::game_stats::user_id.eq(user_id))
            .order(schema::game_stats::played_at.desc())
            .load::<GameStat>(&mut conn)?;

        info!(user_id = %user_id, count = stats.len(), "User stats loaded");
        Ok(stats)
    }

    /// Gets aggregated win/loss/draw counts for a user.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub fn get_aggregated_stats(&self, user_id: i32) -> Result<AggregatedStats, DbError> {
        debug!(user_id = %user_id, "Computing aggregated stats");
        let mut conn = self.connection()?;

        let stats = schema::game_stats::table
            .filter(schema::game_stats::user_id.eq(user_id))
            .load::<GameStat>(&mut conn)?;

        let mut wins = 0;
        let mut losses = 0;
        let mut draws = 0;

        for stat in &stats {
            match stat.outcome().as_str() {
                "win" => wins += 1,
                "loss" => losses += 1,
                "draw" => draws += 1,
                other => warn!(outcome = %other, stat_id = stat.id(), "Unknown outcome value"),
            }
        }

        let total = stats.len() as i32;
        let aggregated = AggregatedStats::new(total, wins, losses, draws);

        info!(
            user_id = %user_id,
            total = %total,
            wins = %wins,
            losses = %losses,
            draws = %draws,
            win_rate = %format!("{:.1}%", aggregated.win_rate()),
            "Aggregated stats computed"
        );

        Ok(aggregated)
    }

    /// Gets game stats filtered by opponent name, ordered most recent first.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub fn get_stats_by_opponent(
        &self,
        user_id: i32,
        opponent_name: &str,
    ) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id = %user_id, opponent = %opponent_name, "Loading stats by opponent");
        let mut conn = self.connection()?;

        let stats = schema::game_stats::table
            .filter(schema::game_stats::user_id.eq(user_id))
            .filter(schema::game_stats::opponent_name.eq(opponent_name))
            .order(schema::game_stats::played_at.desc())
            .load::<GameStat>(&mut conn)?;

        info!(user_id = %user_id, opponent = %opponent_name, count = stats.len(), "Opponent stats loaded");
        Ok(stats)
    }
}
