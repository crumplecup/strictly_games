//! Database repository for game statistics and user profiles.

use std::sync::Arc;

use chrono::Utc;
use elicit_db::{DbError, DbErrorKind, DbKvStore, DbValue};
use elicit_redb::RedbBackend;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::db::{AggregatedStats, GameStat, NewGameStat, User};

const TABLE_USERS: &str = "users";
const TABLE_USERS_BY_NAME: &str = "users_by_name";
const TABLE_GAME_STATS: &str = "game_stats";

/// Database repository for user and game operations.
///
/// Backed by [`RedbBackend`]; the database file is created automatically
/// on first open. Use [`GameRepository::in_memory`] for tests.
pub struct GameRepository {
    backend: Arc<RedbBackend>,
}

impl std::fmt::Debug for GameRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameRepository").finish()
    }
}

impl Clone for GameRepository {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
        }
    }
}

impl GameRepository {
    /// Opens or creates the redb file at `path`.
    ///
    /// The file is created automatically if it does not exist — no migrations
    /// or prior setup required.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the path cannot be opened.
    #[instrument(fields(%path))]
    pub fn open(path: &str) -> Result<Self, DbError> {
        info!(path, "Opening GameRepository");
        let backend = RedbBackend::open(path)?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a transient in-memory repository (for tests).
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the in-memory backend cannot be initialised.
    pub fn in_memory() -> Result<Self, DbError> {
        let backend = RedbBackend::in_memory()?;
        Ok(Self {
            backend: Arc::new(backend),
        })
    }

    /// Creates a new user profile.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if the display name is already taken or a
    /// database error occurs.
    #[instrument(skip(self))]
    pub async fn create_user(&self, display_name: String) -> Result<User, DbError> {
        debug!(display_name = %display_name, "Creating user");

        let name_key = DbValue::Text(display_name.clone());
        if self
            .backend
            .kv_get(TABLE_USERS_BY_NAME, &name_key)
            .await?
            .is_some()
        {
            return Err(DbError::new(DbErrorKind::ConstraintViolation(format!(
                "Display name already taken: '{display_name}'"
            ))));
        }

        let now = Utc::now().to_rfc3339();
        let user = User::new(
            Uuid::new_v4().to_string(),
            display_name.clone(),
            now.clone(),
            now,
        );

        let user_json = serde_json::to_string(&user)
            .map_err(|e| DbError::new(DbErrorKind::Serialization(e.to_string())))?;

        self.backend
            .kv_insert(
                TABLE_USERS,
                DbValue::Text(user.id().clone()),
                DbValue::Text(user_json),
            )
            .await?;

        self.backend
            .kv_insert(
                TABLE_USERS_BY_NAME,
                DbValue::Text(display_name.clone()),
                DbValue::Text(user.id().clone()),
            )
            .await?;

        info!(user_id = %user.id(), %display_name, "User created");
        Ok(user)
    }

    /// Gets a user by display name. Returns `None` if not found.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub async fn get_user_by_name(&self, display_name: &str) -> Result<Option<User>, DbError> {
        debug!(display_name, "Looking up user by name");

        let uuid = match self
            .backend
            .kv_get(TABLE_USERS_BY_NAME, &DbValue::Text(display_name.to_owned()))
            .await?
        {
            Some(DbValue::Text(uuid)) => uuid,
            Some(_) | None => return Ok(None),
        };

        self.get_user_by_id(&uuid).await
    }

    /// Gets a user by UUID. Returns `None` if not found.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, DbError> {
        debug!(user_id, "Looking up user by id");

        match self
            .backend
            .kv_get(TABLE_USERS, &DbValue::Text(user_id.to_owned()))
            .await?
        {
            Some(DbValue::Text(json)) => {
                let user: User = serde_json::from_str(&json)
                    .map_err(|e| DbError::new(DbErrorKind::Serialization(e.to_string())))?;
                debug!(user_id = %user.id(), "User found");
                Ok(Some(user))
            }
            Some(_) | None => Ok(None),
        }
    }

    /// Lists all user profiles, sorted by creation time (oldest first).
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub async fn list_users(&self) -> Result<Vec<User>, DbError> {
        debug!("Listing all users");

        let entries = self.backend.kv_scan(TABLE_USERS).await?;
        let mut users: Vec<User> = entries
            .into_iter()
            .filter_map(|e| match e.value {
                DbValue::Text(json) => serde_json::from_str(&json).ok(),
                _ => None,
            })
            .collect();

        users.sort_by(|a, b| a.created_at().cmp(b.created_at()));
        info!(count = users.len(), "Users listed");
        Ok(users)
    }

    /// Records a completed game result.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self, stat), fields(user_id = %stat.user_id(), game_type = %stat.game_type(), outcome = %stat.outcome()))]
    pub async fn record_game(&self, stat: NewGameStat) -> Result<GameStat, DbError> {
        debug!("Recording game result");

        let game_stat =
            GameStat::from_stat(Uuid::new_v4().to_string(), Utc::now().to_rfc3339(), stat);

        let json = serde_json::to_string(&game_stat)
            .map_err(|e| DbError::new(DbErrorKind::Serialization(e.to_string())))?;

        self.backend
            .kv_insert(
                TABLE_GAME_STATS,
                DbValue::Text(game_stat.id().clone()),
                DbValue::Text(json),
            )
            .await?;

        info!(
            stat_id = %game_stat.id(),
            user_id = %game_stat.user_id(),
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
    pub async fn get_user_stats(&self, user_id: &str) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id, "Loading user stats");

        let mut stats = self.scan_stats_for_user(user_id).await?;
        stats.sort_by(|a, b| b.played_at().cmp(a.played_at()));

        info!(user_id, count = stats.len(), "User stats loaded");
        Ok(stats)
    }

    /// Gets aggregated win/loss/draw counts for a user.
    ///
    /// # Errors
    ///
    /// Returns [`DbError`] if a database error occurs.
    #[instrument(skip(self))]
    pub async fn get_aggregated_stats(&self, user_id: &str) -> Result<AggregatedStats, DbError> {
        debug!(user_id, "Computing aggregated stats");

        let stats = self.scan_stats_for_user(user_id).await?;
        let mut wins = 0;
        let mut losses = 0;
        let mut draws = 0;

        for stat in &stats {
            match stat.outcome().as_str() {
                "win" => wins += 1,
                "loss" => losses += 1,
                "draw" => draws += 1,
                other => warn!(outcome = %other, stat_id = %stat.id(), "Unknown outcome value"),
            }
        }

        let total = stats.len() as i32;
        let aggregated = AggregatedStats::new(total, wins, losses, draws);

        info!(
            user_id,
            total,
            wins,
            losses,
            draws,
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
    pub async fn get_stats_by_opponent(
        &self,
        user_id: &str,
        opponent_name: &str,
    ) -> Result<Vec<GameStat>, DbError> {
        debug!(user_id, opponent = %opponent_name, "Loading stats by opponent");

        let mut stats = self.scan_stats_for_user(user_id).await?;
        stats.retain(|s| s.opponent_name() == opponent_name);
        stats.sort_by(|a, b| b.played_at().cmp(a.played_at()));

        info!(user_id, opponent = %opponent_name, count = stats.len(), "Opponent stats loaded");
        Ok(stats)
    }

    /// Scans all game stats and filters to those belonging to `user_id`.
    #[instrument(skip(self))]
    async fn scan_stats_for_user(&self, user_id: &str) -> Result<Vec<GameStat>, DbError> {
        let entries = self.backend.kv_scan(TABLE_GAME_STATS).await?;
        Ok(entries
            .into_iter()
            .filter_map(|e| match e.value {
                DbValue::Text(json) => serde_json::from_str::<GameStat>(&json).ok(),
                _ => None,
            })
            .filter(|s| s.user_id() == user_id)
            .collect())
    }
}
