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
use elicit_ui::{VerifiedTree, Viewport};
use ratatui::widgets::ListState;
use tracing::{info, instrument};

use crate::lobby::lobby_ir::blackjack_setup_to_verified_tree;
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
    #[instrument(skip(self))]
    fn to_verified_tree(&self, viewport: Viewport) -> VerifiedTree {
        blackjack_setup_to_verified_tree(
            &self.human_name,
            &self.agents,
            &self.seated,
            self.list_state.selected(),
            viewport,
        )
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
