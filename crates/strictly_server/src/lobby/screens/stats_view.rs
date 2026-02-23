//! Statistics view screen — shows win/loss/draw history for a user.

use crossterm::event::{KeyCode, KeyEvent};
use derive_getters::Getters;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use tracing::{debug, info, instrument};

use crate::lobby::screen::{Screen, ScreenTransition};
use crate::{AggregatedStats, GameStat, ProfileService, User};

/// State for the statistics view screen.
#[derive(Debug, Getters)]
pub struct StatsViewScreen {
    current_user: User,
    aggregated: Option<AggregatedStats>,
    recent_games: Vec<GameStat>,
}

impl StatsViewScreen {
    /// Creates a new stats view screen for the given user, loading data immediately.
    #[instrument(skip(current_user, profile_service))]
    pub fn new(current_user: User, profile_service: &ProfileService) -> Self {
        let user_id = *current_user.id();
        debug!(user_id, "Initializing StatsViewScreen");

        let aggregated = profile_service.get_stats(user_id).ok();
        let recent_games = profile_service.get_history(user_id).unwrap_or_default();

        info!(
            user_id,
            total_games = recent_games.len(),
            "StatsViewScreen initialized"
        );

        Self {
            current_user,
            aggregated,
            recent_games,
        }
    }
}

impl Screen for StatsViewScreen {
    #[instrument(skip(self, frame, _profile_service))]
    fn render(&self, frame: &mut Frame, _profile_service: &ProfileService) {
        let area = frame.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(5),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        let title_text = format!("Statistics — {}", self.current_user.display_name());
        let title = Paragraph::new(title_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(title, chunks[0]);

        let summary_text = match &self.aggregated {
            Some(stats) => format!(
                "Games: {}   Wins: {}   Losses: {}   Draws: {}   Win Rate: {:.1}%",
                stats.total_games(),
                stats.wins(),
                stats.losses(),
                stats.draws(),
                stats.win_rate()
            ),
            None => "No statistics available".to_string(),
        };
        let summary = Paragraph::new(summary_text)
            .style(Style::default().fg(Color::Green))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Summary"));
        frame.render_widget(summary, chunks[1]);

        let header = Row::new(vec![
            Cell::from("Opponent").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Game").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Outcome").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Moves").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(Color::Yellow));

        let rows: Vec<Row> = self
            .recent_games
            .iter()
            .take(20)
            .map(|stat| {
                let outcome_color = match stat.outcome().as_str() {
                    "win" => Color::Green,
                    "loss" => Color::Red,
                    "draw" => Color::Yellow,
                    _ => Color::White,
                };
                Row::new(vec![
                    Cell::from(stat.opponent_name().as_str()),
                    Cell::from(stat.game_type().as_str()),
                    Cell::from(stat.outcome().as_str()).style(Style::default().fg(outcome_color)),
                    Cell::from(stat.moves_count().to_string()),
                ])
            })
            .collect();

        let widths = [
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Recent Games (20 most recent)"),
            )
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        frame.render_widget(table, chunks[2]);

        let help = Paragraph::new("Esc / b: Back to Lobby | q: Quit")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[3]);
    }

    #[instrument(skip(self, key, _profile_service))]
    fn handle_key(&mut self, key: KeyEvent, _profile_service: &ProfileService) -> ScreenTransition {
        match key.code {
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                info!("Returning to main lobby from stats");
                ScreenTransition::GoToMainLobby
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => ScreenTransition::Quit,
            _ => ScreenTransition::Stay,
        }
    }
}
