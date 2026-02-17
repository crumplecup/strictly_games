//! In-game screen — shown while a game session is running.
//!
//! This screen acts as a placeholder while the [`LobbyController`] hands off
//! control to the async game loop. It renders a "game in progress" status and
//! returns `GoToMainLobby` on any key press to allow the controller to clean up.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use ratatui::{
    Frame,
    layout::Alignment,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use tracing::{debug, instrument};

use crate::ProfileService;
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
    #[instrument(skip(self, frame, _profile_service))]
    fn render(&self, frame: &mut Frame, _profile_service: &ProfileService) {
        let area = frame.area();

        let (text, color) = if self.game_finished {
            let msg = self.result_message.as_deref().unwrap_or("Game finished.");
            (
                format!("{}\n\nPress any key to return to lobby.", msg),
                Color::Green,
            )
        } else {
            (
                format!(
                    "Game in progress vs {}…\n\nThe game loop is running.\nPress any key to return to lobby.",
                    self.agent_name
                ),
                Color::Yellow,
            )
        };

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("In Game"));

        frame.render_widget(paragraph, area);
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::GoToMainLobby,
        }
    }
}
