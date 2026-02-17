//! Profile selection screen — create or select a user profile.

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

/// State for the profile selection screen.
///
/// Shows a list of existing profiles and an input field for creating a new one.
#[derive(Debug, Getters)]
pub struct ProfileSelectScreen {
    users: Vec<User>,
    list_state: ListState,
    new_name_input: String,
    input_mode: bool,
    error_message: Option<String>,
    selected_user_id: Option<i32>,
}

impl ProfileSelectScreen {
    /// Creates a new profile select screen, loading existing users.
    #[instrument(skip(profile_service))]
    pub fn new(profile_service: &ProfileService) -> Self {
        debug!("Initializing ProfileSelectScreen");
        let users = profile_service
            .repository()
            .list_users()
            .unwrap_or_default();
        info!(user_count = users.len(), "ProfileSelectScreen initialized");
        let mut state = ListState::default();
        if !users.is_empty() {
            state.select(Some(0));
        }
        Self {
            users,
            list_state: state,
            new_name_input: String::new(),
            input_mode: false,
            error_message: None,
            selected_user_id: None,
        }
    }

    /// Moves the list selection up by one.
    #[instrument(skip(self))]
    fn select_previous(&mut self) {
        if self.users.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => self.users.len() - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves the list selection down by one.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        if self.users.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.users.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Confirms the selected profile and returns the selected user id.
    #[instrument(skip(self))]
    fn confirm_selection(&mut self) -> Option<i32> {
        if let Some(idx) = self.list_state.selected()
            && let Some(user) = self.users.get(idx)
        {
            let id = *user.id();
            info!(user_id = id, display_name = %user.display_name(), "Profile selected");
            self.selected_user_id = Some(id);
            return Some(id);
        }
        None
    }

    /// Creates a new user profile from the current input.
    #[instrument(skip(self, profile_service))]
    fn create_profile(&mut self, profile_service: &ProfileService) -> Option<i32> {
        let name = self.new_name_input.trim().to_string();
        if name.is_empty() {
            self.error_message = Some("Name cannot be empty".to_string());
            return None;
        }

        match profile_service.get_or_create_user(name.clone()) {
            Ok(user) => {
                info!(user_id = user.id(), display_name = %name, "Profile created");
                let id = *user.id();
                self.users = profile_service
                    .repository()
                    .list_users()
                    .unwrap_or_default();
                let pos = self
                    .users
                    .iter()
                    .position(|u| u.id() == user.id())
                    .unwrap_or(0);
                self.list_state.select(Some(pos));
                self.new_name_input.clear();
                self.input_mode = false;
                self.error_message = None;
                self.selected_user_id = Some(id);
                Some(id)
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to create profile: {}", e.message));
                None
            }
        }
    }
}

impl Screen for ProfileSelectScreen {
    #[instrument(skip(self, frame, _profile_service))]
    fn render(&self, frame: &mut Frame, _profile_service: &ProfileService) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("Select or Create Profile")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = self
            .users
            .iter()
            .map(|u| ListItem::new(u.display_name().as_str()))
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Profiles"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = self.list_state;
        frame.render_stateful_widget(list, chunks[1], &mut list_state);

        let input_title = if self.input_mode {
            "New Profile Name (Enter to confirm, Esc to cancel)"
        } else {
            "Press 'n' to create new profile"
        };
        let input_style = if self.input_mode {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let input = Paragraph::new(self.new_name_input.as_str())
            .style(input_style)
            .block(Block::default().borders(Borders::ALL).title(input_title));
        frame.render_widget(input, chunks[2]);

        let error_text = self.error_message.as_deref().unwrap_or("");
        let error = Paragraph::new(error_text)
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(error, chunks[3]);

        let help_text = if self.input_mode {
            "Type name | Enter: Confirm | Esc: Cancel"
        } else {
            "↑↓: Select | Enter: Confirm | n: New | q: Quit"
        };
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[4]);
    }

    #[instrument(skip(self, key, profile_service))]
    fn handle_key(&mut self, key: KeyEvent, profile_service: &ProfileService) -> ScreenTransition {
        if self.input_mode {
            match key.code {
                KeyCode::Char(c) => {
                    self.new_name_input.push(c);
                    ScreenTransition::Stay
                }
                KeyCode::Backspace => {
                    self.new_name_input.pop();
                    ScreenTransition::Stay
                }
                KeyCode::Enter => {
                    if self.create_profile(profile_service).is_some() {
                        ScreenTransition::GoToMainLobby
                    } else {
                        ScreenTransition::Stay
                    }
                }
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.new_name_input.clear();
                    self.error_message = None;
                    ScreenTransition::Stay
                }
                _ => ScreenTransition::Stay,
            }
        } else {
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
                    if self.confirm_selection().is_some() {
                        ScreenTransition::GoToMainLobby
                    } else if !self.users.is_empty() {
                        ScreenTransition::Stay
                    } else {
                        self.input_mode = true;
                        ScreenTransition::Stay
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.input_mode = true;
                    self.error_message = None;
                    ScreenTransition::Stay
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
                _ => ScreenTransition::Stay,
            }
        }
    }
}
