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

/// Number of settings items in the list.
const SETTINGS_COUNT: usize = 2;

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

    /// Moves selection up.
    #[instrument(skip(self))]
    fn select_prev(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(i.saturating_sub(1)));
    }

    /// Moves selection down.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        self.list_state
            .select(Some((i + 1).min(SETTINGS_COUNT - 1)));
    }

    /// Toggles the currently selected setting.
    #[instrument(skip(self))]
    fn toggle_selected(&mut self) {
        match self.list_state.selected().unwrap_or(0) {
            0 => {
                self.settings.first_player = self.settings.first_player.toggle();
                info!(
                    first_player = %self.settings.first_player.label(),
                    "Toggled first player setting"
                );
            }
            1 => {
                self.settings.show_typestate_graph = !self.settings.show_typestate_graph;
                info!(
                    show_typestate_graph = self.settings.show_typestate_graph,
                    "Toggled typestate graph setting"
                );
            }
            _ => {}
        }
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

        let checkbox = |checked: bool| if checked { "[✓]" } else { "[ ]" };

        let items = vec![
            ListItem::new(format!(
                "Who Goes First?    [ {} ]",
                self.settings.first_player.label()
            )),
            ListItem::new(format!(
                "Show Typestate Graph   {}",
                checkbox(self.settings.show_typestate_graph)
            )),
        ];

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

        let help = Paragraph::new("↑↓: Navigate | ←→ / Enter / Space: Toggle | Esc: Back")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[2]);
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
            KeyCode::Enter | KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                self.toggle_selected();
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
