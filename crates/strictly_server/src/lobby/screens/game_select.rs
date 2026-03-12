//! Game selection screen — choose which game to play.

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
use crate::lobby::settings::GameType;

/// State for the game selection screen.
#[derive(Debug)]
pub struct GameSelectScreen {
    list_state: ListState,
}

impl GameSelectScreen {
    /// Creates a new game selection screen with the given game pre-selected.
    #[instrument]
    pub fn new(current: GameType) -> Self {
        debug!(game = %current.label(), "Initializing GameSelectScreen");
        let idx = GameType::all()
            .iter()
            .position(|&g| g == current)
            .unwrap_or(0);
        let mut list_state = ListState::default();
        list_state.select(Some(idx));
        Self { list_state }
    }

    /// Returns the currently highlighted game type.
    #[instrument(skip(self))]
    fn selected_game(&self) -> GameType {
        let games = GameType::all();
        let idx = self.list_state.selected().unwrap_or(0);
        games[idx.min(games.len() - 1)]
    }

    /// Moves selection up.
    #[instrument(skip(self))]
    fn select_prev(&mut self) {
        let count = GameType::all().len();
        let i = match self.list_state.selected() {
            Some(i) if i > 0 => i - 1,
            _ => count - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves selection down.
    #[instrument(skip(self))]
    fn select_next(&mut self) {
        let count = GameType::all().len();
        let i = self.list_state.selected().map_or(0, |i| (i + 1) % count);
        self.list_state.select(Some(i));
    }
}

impl Screen for GameSelectScreen {
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

        let title = Paragraph::new("Select Game")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = GameType::all()
            .iter()
            .map(|g| ListItem::new(g.label()))
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Games"))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut list_state = self.list_state;
        frame.render_stateful_widget(list, chunks[1], &mut list_state);

        let help = Paragraph::new("↑↓: Navigate | Enter: Select | Esc: Back")
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
            KeyCode::Enter => {
                let game = self.selected_game();
                info!(game = %game.label(), "Game selected");
                ScreenTransition::GameSelected { game }
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                info!("Leaving game select screen");
                ScreenTransition::GoToMainLobby
            }
            _ => ScreenTransition::Stay,
        }
    }
}
