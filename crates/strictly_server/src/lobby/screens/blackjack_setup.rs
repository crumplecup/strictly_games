//! Blackjack table setup screen — configure seats before play begins.
//!
//! Presents the human player's seat (always present) and a list of AI agents
//! from the library that can be toggled in or out.  Up to [`MAX_AGENTS`]
//! agents may be added, giving a maximum of [`MAX_AGENTS`] + 1 seats (the
//! human plus up to 3 agents).
//!
//! Controls:
//! - `↑` / `↓` — move focus through the agent list
//! - `Space` / `Enter` on an agent — toggle that agent in / out of the table
//! - `s` — start the game (emits [`ScreenTransition::GoToBlackjackTable`])
//! - `Esc` / `q` — cancel and return to the main lobby

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use tracing::{info, instrument};

use crate::lobby::screen::{Screen, ScreenTransition};
use crate::lobby::settings::{PlayerKind, PlayerSlot};
use crate::{AgentConfig, AgentLibrary, ProfileService};

/// Maximum number of AI agents that may join the table alongside the human.
pub const MAX_AGENTS: usize = 3;

/// Default starting bankroll for every seat.
const DEFAULT_BANKROLL: u64 = 1_000;

/// State for the blackjack table setup screen.
#[derive(Debug)]
pub struct BlackjackSetupScreen {
    human_name: String,
    agents: Vec<AgentConfig>,
    /// `true` at index `i` means agent `i` is seated at the table.
    seated: Vec<bool>,
    list_state: ListState,
}

impl BlackjackSetupScreen {
    /// Creates a new setup screen for `human_name` showing agents from `library`.
    #[instrument(skip(human_name, library), fields(human_name = %human_name.as_ref()))]
    pub fn new(human_name: impl Into<String> + AsRef<str>, library: &AgentLibrary) -> Self {
        let human_name = human_name.into();
        let agents: Vec<AgentConfig> = library.agents().to_vec();
        let seated = vec![false; agents.len()];
        let mut list_state = ListState::default();
        if !agents.is_empty() {
            list_state.select(Some(0));
        }
        info!(
            human = %human_name,
            num_agents = agents.len(),
            "Initializing BlackjackSetupScreen"
        );
        Self {
            human_name,
            agents,
            seated,
            list_state,
        }
    }

    /// Number of agents currently seated.
    fn seated_count(&self) -> usize {
        self.seated.iter().filter(|&&s| s).count()
    }

    /// Toggles the agent at the selected index in/out of the table.
    #[instrument(skip(self))]
    fn toggle_selected(&mut self) {
        let Some(i) = self.list_state.selected() else {
            return;
        };
        if self.seated[i] {
            self.seated[i] = false;
            info!(agent = %self.agents[i].name(), "Removed agent from table");
        } else if self.seated_count() < MAX_AGENTS {
            self.seated[i] = true;
            info!(agent = %self.agents[i].name(), "Added agent to table");
        } else {
            info!(max = MAX_AGENTS, "Table is full; cannot add more agents");
        }
    }

    /// Moves the list selection up.
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

    /// Moves the list selection down.
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

    /// Builds the [`Vec<PlayerSlot>`] for the configured table.
    ///
    /// The human's slot is always first; seated agents follow in list order.
    #[instrument(skip(self))]
    fn build_player_slots(&self) -> Vec<PlayerSlot> {
        let mut slots = Vec::with_capacity(1 + self.seated_count());
        slots.push(PlayerSlot {
            name: self.human_name.clone(),
            bankroll: DEFAULT_BANKROLL,
            kind: PlayerKind::Human,
        });
        for (agent, &seated) in self.agents.iter().zip(self.seated.iter()) {
            if seated {
                slots.push(PlayerSlot {
                    name: agent.name().to_string(),
                    bankroll: DEFAULT_BANKROLL,
                    kind: PlayerKind::Agent(agent.clone()),
                });
            }
        }
        slots
    }
}

impl Screen for BlackjackSetupScreen {
    #[instrument(skip(self, frame, _profile_service))]
    fn render(&self, frame: &mut Frame, _profile_service: &ProfileService) {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // title
                Constraint::Length(3), // human seat
                Constraint::Min(5),    // agent list
                Constraint::Length(3), // status / help
            ])
            .split(area);

        // ── Title ───────────────────────────────────────────────────────────
        let title = Paragraph::new("♠  Blackjack Table Setup  ♠")
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(title, chunks[0]);

        // ── Human seat ──────────────────────────────────────────────────────
        let seat_text = format!(
            "  ✔  {} (You)   —   bankroll: {} chips",
            self.human_name, DEFAULT_BANKROLL
        );
        let seat_widget = Paragraph::new(seat_text)
            .style(Style::default().fg(Color::Cyan))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Your Seat ")
                    .border_style(Style::default().fg(Color::Cyan)),
            );
        frame.render_widget(seat_widget, chunks[1]);

        // ── Agent list ──────────────────────────────────────────────────────
        let items: Vec<ListItem> = self
            .agents
            .iter()
            .enumerate()
            .map(|(i, agent)| {
                let check = if self.seated[i] { "✔" } else { "○" };
                let color = if self.seated[i] {
                    Color::Green
                } else {
                    Color::Gray
                };
                ListItem::new(format!("  {}  {}", check, agent.name()))
                    .style(Style::default().fg(color))
            })
            .collect();

        let agent_count_label = if self.agents.is_empty() {
            " No agents in library ".to_string()
        } else {
            format!(
                " Add AI Agents ({}/{} selected) ",
                self.seated_count(),
                MAX_AGENTS
            )
        };

        let agent_list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(agent_count_label)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            );
        frame.render_stateful_widget(agent_list, chunks[2], &mut self.list_state.clone());

        // ── Help bar ────────────────────────────────────────────────────────
        let help =
            Paragraph::new("↑↓: Navigate  Space/Enter: Toggle agent  s: Start game  Esc/q: Back")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::TOP));
        frame.render_widget(help, chunks[3]);
    }

    #[instrument(skip(self, _profile_service))]
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
            KeyCode::Char(' ') | KeyCode::Enter => {
                if !self.agents.is_empty() {
                    self.toggle_selected();
                }
                ScreenTransition::Stay
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                let players = self.build_player_slots();
                info!(
                    seats = players.len(),
                    agents = self.seated_count(),
                    "Starting blackjack table"
                );
                ScreenTransition::GoToBlackjackTable { players }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                ScreenTransition::GoToMainLobby
            }
            _ => ScreenTransition::Stay,
        }
    }
}
