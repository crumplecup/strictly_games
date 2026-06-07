//! Game selection screen — choose which game to play.

use crossterm::event::{KeyCode, KeyEvent};
use elicit_ui::{VerifiedTree, Viewport};
use ratatui::widgets::ListState;
use tracing::{debug, info, instrument};

use crate::ProfileService;
use crate::lobby::lobby_ir::game_select_to_verified_tree;
use crate::lobby::screen::{Screen, ScreenTransition};
use crate::lobby::settings::GameType;

/// State for the game selection screen.
#[derive(Debug)]
pub struct GameSelectScreen {
    list_state: ListState,
}

impl GameSelectScreen {
    /// Creates a new game selection screen with the given game pre-selected.
    #[instrument]
    pub fn new(current: GameType) -> Self {
        debug!(game = %current.label(), "Initializing GameSelectScreen");
        let idx = GameType::all()
            .iter()
            .position(|&g| g == current)
            .unwrap_or(0);
        let mut list_state = ListState::default();
        list_state.select(Some(idx));
        Self { list_state }
    }

    /// Returns the currently highlighted game type.
    #[instrument(skip(self))]
    fn selected_game(&self) -> GameType {
        let games = GameType::all();
        let idx = self.list_state.selected().unwrap_or(0);
        games[idx.min(games.len() - 1)]
    }

    /// Moves selection up.
    #[instrument(skip(self))]
    fn select_prev(&mut self) {
        let count = GameType::all().len();
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => count - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves selection down.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        let count = GameType::all().len();
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % count);
        self.list_state.select(Some(i));
    }
}

impl Screen for GameSelectScreen {
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        game_select_to_verified_tree(self.list_state.selected().unwrap_or(0), viewport)
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Up => {
                self.select_prev();
                ScreenTransition::Stay
            }
            KeyCode::Down => {
                self.select_next();
                ScreenTransition::Stay
            }
            KeyCode::Enter => {
                let game = self.selected_game();
                info!(game = %game.label(), "Game selected");
                ScreenTransition::GameSelected { game }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                info!("Leaving game select screen");
                ScreenTransition::GoToMainLobby
            }
            _ => ScreenTransition::Stay,
        }
    }
}
