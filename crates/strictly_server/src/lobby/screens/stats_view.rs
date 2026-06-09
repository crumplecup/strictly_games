//! Statistics view screen — shows win/loss/draw history for a user.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use elicit_ui::{VerifiedTree, Viewport};
use tracing::{debug, info, instrument};

use crate::lobby::lobby_ir::stats_view_to_verified_tree;
use crate::lobby::screen::{Screen, ScreenTransition};
use crate::{AggregatedStats, GameStat, ProfileService, User};

/// State for the statistics view screen.
#[derive(Debug, Getters)]
pub struct StatsViewScreen {
    current_user: User,
    aggregated: Option<AggregatedStats>,
    recent_games: Vec<GameStat>,
}

impl StatsViewScreen {
    /// Creates a new stats view screen for the given user, loading data immediately.
    #[instrument(skip(current_user, profile_service))]
    pub fn new(current_user: User, profile_service: &ProfileService) -> Self {
        let user_id = current_user.id().clone();
        debug!(user_id = %user_id, "Initializing StatsViewScreen");

        let handle = tokio::runtime::Handle::current();
        let aggregated =
            tokio::task::block_in_place(|| handle.block_on(profile_service.get_stats(&user_id)))
                .ok();
        let recent_games =
            tokio::task::block_in_place(|| handle.block_on(profile_service.get_history(&user_id)))
                .unwrap_or_default();

        info!(
            user_id,
            total_games = recent_games.len(),
            "StatsViewScreen initialized"
        );

        Self {
            current_user,
            aggregated,
            recent_games,
        }
    }
}

impl Screen for StatsViewScreen {
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        stats_view_to_verified_tree(
            &self.current_user,
            self.aggregated.as_ref(),
            &self.recent_games,
            viewport,
        )
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                info!("Returning to main lobby from stats");
                ScreenTransition::GoToMainLobby
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::Stay,
        }
    }
}
