//! Main lobby screen — hub for navigation after profile selection.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use elicit_ui::{VerifiedTree, Viewport};
use ratatui::widgets::ListState;
use tracing::{debug, info, instrument};

use crate::lobby::lobby_ir::main_lobby_to_verified_tree;
use crate::lobby::screen::{Screen, ScreenTransition};
use crate::lobby::settings::GameType;
use crate::{AggregatedStats, ProfileService, User};

/// Menu options available in the main lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LobbyOption {
    PlayGame,
    SelectGame,
    ViewStats,
    ChangeProfile,
    Settings,
    Quit,
}

impl LobbyOption {
    #[instrument]
    fn all() -> &'static [LobbyOption] {
        &[
            Self::PlayGame,
            Self::SelectGame,
            Self::ViewStats,
            Self::ChangeProfile,
            Self::Settings,
            Self::Quit,
        ]
    }
}

/// State for the main lobby screen.
#[derive(Debug, Getters)]
pub struct MainLobbyScreen {
    current_user: User,
    selected_game: GameType,
    list_state: ListState,
    /// Cached aggregated stats, loaded once in the constructor.
    stats: Option<AggregatedStats>,
}

impl MainLobbyScreen {
    /// Creates a main lobby screen for the given user and game selection.
    ///
    /// Loads aggregated stats in the constructor via `block_in_place` so
    /// that `render` remains a sync method.
    #[instrument(skip(current_user, profile_service))]
    pub fn with_game(
        current_user: User,
        selected_game: GameType,
        profile_service: &ProfileService,
    ) -> Self {
        debug!(user_id = %current_user.id(), game = %selected_game.label(), "Initializing MainLobbyScreen");
        let user_id = current_user.id().clone();
        let handle = tokio::runtime::Handle::current();
        let stats = tokio::task::block_in_place(|| handle.block_on(profile_service.get_stats(&user_id))).ok();
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            current_user,
            selected_game,
            list_state: state,
            stats,
        }
    }

    /// Moves selection up.
    #[instrument(skip(self))]
    fn select_previous(&mut self) {
        let count = LobbyOption::all().len();
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => count - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves selection down.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        let count = LobbyOption::all().len();
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % count,
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Returns the currently selected menu option.
    #[instrument(skip(self))]
    fn selected_option(&self) -> LobbyOption {
        let options = LobbyOption::all();
        let idx = self.list_state.selected().unwrap_or(0);
        options[idx.min(options.len() - 1)]
    }
}

impl Screen for MainLobbyScreen {
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        main_lobby_to_verified_tree(
            &self.current_user,
            self.stats.as_ref(),
            self.selected_game,
            self.list_state.selected().unwrap_or(0),
            viewport,
        )
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Up => {
                self.select_previous();
                ScreenTransition::Stay
            }
            KeyCode::Down => {
                self.select_next();
                ScreenTransition::Stay
            }
            KeyCode::Enter => {
                let option = self.selected_option();
                info!(option = ?option, "Lobby option selected");
                match option {
                    LobbyOption::PlayGame => {
                        if self.selected_game == GameType::Blackjack {
                            ScreenTransition::GoToBlackjackSetup
                        } else {
                            ScreenTransition::GoToAgentSelect
                        }
                    }
                    LobbyOption::SelectGame => ScreenTransition::GoToGameSelect,
                    LobbyOption::ViewStats => ScreenTransition::GoToStatsView,
                    LobbyOption::ChangeProfile => ScreenTransition::GoToProfileSelect,
                    LobbyOption::Settings => ScreenTransition::GoToSettings,
                    LobbyOption::Quit => ScreenTransition::Quit,
                }
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::Stay,
        }
    }
}
