//! Profile management business logic layer.

use tracing::{debug, info, instrument};

use crate::{AggregatedStats, DbError, GameOutcome, GameRepository, GameStat, NewGameStat, User};

/// Service layer for user profile operations.
///
/// Wraps [`GameRepository`] with higher-level business logic such as
/// get-or-create semantics and game result recording.
#[derive(Debug, Clone)]
pub struct ProfileService {
    repository: GameRepository,
}

impl ProfileService {
    /// Creates a new profile service backed by the given repository.
    #[instrument(skip(repository))]
    pub fn new(repository: GameRepository) -> Self {
        info!("Creating ProfileService");
        Self { repository }
    }

    /// Returns the underlying repository.
    #[instrument(skip(self))]
    pub fn repository(&self) -> &GameRepository {
        &self.repository
    }

    /// Returns an existing user by name or creates one if not found.
    #[instrument(skip(self))]
    pub fn get_or_create_user(&self, display_name: String) -> Result<User, DbError> {
        debug!(display_name = %display_name, "Getting or creating user");

        if let Some(user) = self.repository.get_user_by_name(&display_name)? {
            info!(user_id = user.id(), "Existing user found");
            return Ok(user);
        }

        info!(display_name = %display_name, "Creating new user");
        self.repository.create_user(display_name)
    }

    /// Records a completed game result for a user.
    #[instrument(skip(self))]
    pub fn record_game_result(
        &self,
        user_id: i32,
        opponent_name: String,
        game_type: String,
        outcome: GameOutcome,
        moves_count: i32,
        session_id: String,
    ) -> Result<GameStat, DbError> {
        debug!(
            user_id = %user_id,
            opponent = %opponent_name,
            game_type = %game_type,
            outcome = ?outcome,
            "Recording game result"
        );

        let stat = NewGameStat::new(
            user_id,
            opponent_name,
            game_type,
            outcome.to_db_string().to_string(),
            moves_count,
            session_id,
        );

        let recorded = self.repository.record_game(stat)?;
        info!(stat_id = recorded.id(), "Game result recorded");
        Ok(recorded)
    }

    /// Returns aggregated stats (wins/losses/draws) for a user.
    #[instrument(skip(self))]
    pub fn get_stats(&self, user_id: i32) -> Result<AggregatedStats, DbError> {
        debug!(user_id = %user_id, "Getting aggregated stats");
        self.repository.get_aggregated_stats(user_id)
    }

    /// Returns all game stats for a user, most recent first.
    #[instrument(skip(self))]
    pub fn get_history(&self, user_id: i32) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id = %user_id, "Getting game history");
        self.repository.get_user_stats(user_id)
    }

    /// Returns game stats against a specific opponent, most recent first.
    #[instrument(skip(self))]
    pub fn get_history_vs(
        &self,
        user_id: i32,
        opponent_name: &str,
    ) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id = %user_id, opponent = %opponent_name, "Getting opponent history");
        self.repository
            .get_stats_by_opponent(user_id, opponent_name)
    }
}
