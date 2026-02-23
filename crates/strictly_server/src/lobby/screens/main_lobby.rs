//! Main lobby screen — hub for navigation after profile selection.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use tracing::{debug, info, instrument};

use crate::lobby::screen::{Screen, ScreenTransition};
use crate::{ProfileService, User};

/// Menu options available in the main lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LobbyOption {
    PlayGame,
    ViewStats,
    ChangeProfile,
    Settings,
    Quit,
}

impl LobbyOption {
    #[instrument]
    fn label(self) -> &'static str {
        match self {
            Self::PlayGame => "Play Game",
            Self::ViewStats => "View Statistics",
            Self::ChangeProfile => "Change Profile",
            Self::Settings => "Settings",
            Self::Quit => "Quit",
        }
    }

    #[instrument]
    fn all() -> &'static [LobbyOption] {
        &[
            Self::PlayGame,
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
    list_state: ListState,
}

impl MainLobbyScreen {
    /// Creates a new main lobby screen for the given user.
    #[instrument(skip(current_user))]
    pub fn new(current_user: User) -> Self {
        debug!(user_id = current_user.id(), "Initializing MainLobbyScreen");
        let mut state = ListState::default();
        state.select(Some(0));
        Self {
            current_user,
            list_state: state,
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
    #[instrument(skip(self, frame, profile_service))]
    fn render(&self, frame: &mut Frame, profile_service: &ProfileService) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("Strictly Games — Lobby")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let stats_text = match profile_service.get_stats(*self.current_user.id()) {
            Ok(stats) => format!(
                "Player: {}   W:{} / L:{} / D:{}   Win rate: {:.1}%",
                self.current_user.display_name(),
                stats.wins(),
                stats.losses(),
                stats.draws(),
                stats.win_rate()
            ),
            Err(_) => format!("Player: {}", self.current_user.display_name()),
        };
        let profile_bar = Paragraph::new(stats_text)
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(profile_bar, chunks[1]);

        let items: Vec<ListItem> = LobbyOption::all()
            .iter()
            .map(|opt| ListItem::new(opt.label()))
            .collect();

        let menu = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Menu"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = self.list_state;
        frame.render_stateful_widget(menu, chunks[2], &mut list_state);

        let help = Paragraph::new("↑↓: Navigate | Enter: Select | q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[3]);
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
                    LobbyOption::PlayGame => ScreenTransition::GoToAgentSelect,
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
