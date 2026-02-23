//! Settings screen — configure lobby preferences such as who goes first.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use tracing::{debug, info, instrument};

use crate::ProfileService;
use crate::lobby::screen::{Screen, ScreenTransition};
use crate::lobby::settings::LobbySettings;

/// State for the settings screen.
#[derive(Debug)]
pub struct SettingsScreen {
    settings: LobbySettings,
    list_state: ListState,
}

impl SettingsScreen {
    /// Creates a new settings screen pre-populated with the current settings.
    #[instrument(skip(settings))]
    pub fn new(settings: LobbySettings) -> Self {
        debug!("Initializing SettingsScreen");
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            settings,
            list_state,
        }
    }

    /// Returns the current settings (called by the controller on transition out).
    #[instrument(skip(self))]
    pub fn settings(&self) -> LobbySettings {
        self.settings
    }

    /// Toggles the "Who Goes First?" setting.
    #[instrument(skip(self))]
    fn toggle_first_player(&mut self) {
        self.settings.first_player = self.settings.first_player.toggle();
        info!(
            first_player = %self.settings.first_player.label(),
            "Toggled first player setting"
        );
    }
}

impl Screen for SettingsScreen {
    #[instrument(skip(self, frame, _profile_service))]
    fn render(&self, frame: &mut Frame, _profile_service: &ProfileService) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("Settings")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let first_player_label = format!(
            "Who Goes First?    [ {} ]",
            self.settings.first_player.label()
        );
        let items = vec![ListItem::new(first_player_label)];

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Preferences"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = self.list_state;
        frame.render_stateful_widget(list, chunks[1], &mut list_state);

        let help = Paragraph::new("←→ / Enter: Toggle | Esc: Back")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[2]);
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Enter | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                self.toggle_first_player();
                ScreenTransition::Stay
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                info!("Leaving settings screen");
                ScreenTransition::GoToMainLobby
            }
            _ => ScreenTransition::Stay,
        }
    }
}
