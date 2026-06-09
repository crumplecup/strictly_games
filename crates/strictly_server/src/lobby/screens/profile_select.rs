//! Profile selection screen — create or select a user profile.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use elicit_ui::{VerifiedTree, Viewport};
use ratatui::widgets::ListState;
use tracing::{debug, info, instrument};

use crate::lobby::lobby_ir::profile_select_to_verified_tree;
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
    selected_user_id: Option<String>,
}

impl ProfileSelectScreen {
    /// Creates a new profile select screen, loading existing users.
    #[instrument(skip(profile_service))]
    pub fn new(profile_service: &ProfileService) -> Self {
        debug!("Initializing ProfileSelectScreen");
        let handle = tokio::runtime::Handle::current();
        let users = tokio::task::block_in_place(|| {
            handle.block_on(profile_service.repository().list_users())
        })
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
    fn confirm_selection(&mut self) -> Option<String> {
        if let Some(idx) = self.list_state.selected()
            && let Some(user) = self.users.get(idx)
        {
            let id = user.id().clone();
            info!(user_id = %id, display_name = %user.display_name(), "Profile selected");
            self.selected_user_id = Some(id.clone());
            return Some(id);
        }
        None
    }

    /// Creates a new user profile from the current input.
    #[instrument(skip(self, profile_service))]
    fn create_profile(&mut self, profile_service: &ProfileService) -> Option<String> {
        let name = self.new_name_input.trim().to_string();
        if name.is_empty() {
            self.error_message = Some("Name cannot be empty".to_string());
            return None;
        }

        let handle = tokio::runtime::Handle::current();
        match tokio::task::block_in_place(|| {
            handle.block_on(profile_service.get_or_create_user(name.clone()))
        }) {
            Ok(user) => {
                info!(user_id = %user.id(), display_name = %name, "Profile created");
                let id = user.id().clone();
                let new_users = tokio::task::block_in_place(|| {
                    handle.block_on(profile_service.repository().list_users())
                })
                .unwrap_or_default();
                let pos = new_users.iter().position(|u| u.id() == &id).unwrap_or(0);
                self.users = new_users;
                self.list_state.select(Some(pos));
                self.new_name_input.clear();
                self.input_mode = false;
                self.error_message = None;
                self.selected_user_id = Some(id.clone());
                Some(id)
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to create profile: {e}"));
                None
            }
        }
    }
}

impl Screen for ProfileSelectScreen {
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        profile_select_to_verified_tree(
            &self.users,
            self.list_state.selected(),
            self.input_mode,
            &self.new_name_input,
            self.error_message.as_deref(),
            viewport,
        )
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
