//! Agent selection screen — choose an AI opponent from the library.

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
use crate::{AgentConfig, AgentLibrary, ProfileService};

/// State for the agent selection screen.
#[derive(Debug, Getters)]
pub struct AgentSelectScreen {
    agents: Vec<AgentConfig>,
    list_state: ListState,
}

impl AgentSelectScreen {
    /// Creates a new agent select screen from the given library.
    #[instrument(skip(library))]
    pub fn new(library: &AgentLibrary) -> Self {
        let agents: Vec<AgentConfig> = library.agents().to_vec();
        info!(agent_count = agents.len(), "Initializing AgentSelectScreen");
        let mut state = ListState::default();
        if !agents.is_empty() {
            state.select(Some(0));
        }
        Self {
            agents,
            list_state: state,
        }
    }

    /// Moves selection up.
    #[instrument(skip(self))]
    fn select_previous(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => self.agents.len() - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves selection down.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.agents.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Returns the currently selected agent name.
    #[instrument(skip(self))]
    fn selected_agent_name(&self) -> Option<String> {
        self.list_state
            .selected()
            .and_then(|i| self.agents.get(i))
            .map(|a| {
                debug!(name = %a.name(), "Agent selected");
                a.name().clone()
            })
    }
}

impl Screen for AgentSelectScreen {
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

        let title = Paragraph::new("Select AI Opponent")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = if self.agents.is_empty() {
            vec![ListItem::new(
                "No agents found — check your config directory",
            )]
        } else {
            self.agents
                .iter()
                .map(|a| {
                    let label =
                        format!("{} ({:?} / {})", a.name(), a.llm_provider(), a.llm_model());
                    ListItem::new(label)
                })
                .collect()
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = self.list_state;
        frame.render_stateful_widget(list, chunks[1], &mut list_state);

        let help = Paragraph::new("↑↓: Select | Enter: Start Game | Esc: Back | q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[2]);
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
                if let Some(name) = self.selected_agent_name() {
                    info!(agent_name = %name, "Starting game with agent");
                    ScreenTransition::GoToInGame { agent_name: name }
                } else {
                    ScreenTransition::Stay
                }
            }
            KeyCode::Esc => ScreenTransition::GoToMainLobby,
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::Stay,
        }
    }
}
