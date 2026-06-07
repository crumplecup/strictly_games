//! In-game screen — shown while a game session is running.
//!
//! This screen acts as a placeholder while the [`LobbyController`] hands off
//! control to the async game loop. It renders a "game in progress" status and
//! returns `GoToMainLobby` on any key press to allow the controller to clean up.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use elicit_ui::{VerifiedTree, Viewport};
use tracing::{debug, instrument};

use crate::ProfileService;
use crate::lobby::lobby_ir::in_game_to_verified_tree;
use crate::lobby::screen::{Screen, ScreenTransition};

/// In-game screen shown during an active game session.
#[derive(Debug, Getters)]
pub struct InGameScreen {
    agent_name: String,
    game_finished: bool,
    result_message: Option<String>,
}

impl InGameScreen {
    /// Creates a new in-game screen for the given agent.
    #[instrument(skip(agent_name))]
    pub fn new(agent_name: String) -> Self {
        debug!(agent_name = %agent_name, "Initializing InGameScreen");
        Self {
            agent_name,
            game_finished: false,
            result_message: None,
        }
    }
}

impl Screen for InGameScreen {
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        in_game_to_verified_tree(
            &self.agent_name,
            self.game_finished,
            self.result_message.as_deref(),
            viewport,
        )
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::GoToMainLobby,
        }
    }
}
