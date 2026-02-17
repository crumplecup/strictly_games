//! Screen trait and transition type for the lobby state machine.

use crossterm::event::KeyEvent;
use ratatui::Frame;

use crate::ProfileService;

/// The result of handling an input event on a screen.
///
/// Screens return this from [`Screen::handle_key`] to drive the
/// [`LobbyController`](crate::LobbyController) state machine.
#[derive(Debug, Clone)]
pub enum ScreenTransition {
    /// Stay on the current screen — no state change.
    Stay,
    /// Navigate to the profile selection screen.
    GoToProfileSelect,
    /// Navigate to the main lobby screen.
    GoToMainLobby,
    /// Navigate to the agent selection screen.
    GoToAgentSelect,
    /// Navigate to the statistics view for the current user.
    GoToStatsView,
    /// Start an in-game session with the selected agent.
    GoToInGame {
        /// Name of the selected agent config to use as the AI opponent.
        agent_name: String,
    },
    /// Exit the lobby application cleanly.
    Quit,
}

/// Trait implemented by each screen in the lobby state machine.
///
/// Each screen owns its own state, renders its UI, and handles key events.
/// The controller calls these methods in the event loop.
pub trait Screen {
    /// Renders the screen into the provided [`Frame`].
    fn render(&self, frame: &mut Frame, profile_service: &ProfileService);

    /// Handles a key event and returns the resulting [`ScreenTransition`].
    fn handle_key(&mut self, key: KeyEvent, profile_service: &ProfileService) -> ScreenTransition;
}
