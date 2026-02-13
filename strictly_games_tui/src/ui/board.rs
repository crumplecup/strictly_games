//! Tic-tac-toe board rendering.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};
use strictly_games::games::tictactoe::{types, Game, Player};

use types::{Board, Square};

/// Renders the tic-tac-toe board.
pub fn render_board(f: &mut Frame, area: Rect, game: &Game) {
    let state = game.state();
    let board = state.board();
    let board_area = center_rect(area, 40, 12);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(board_area);

    render_row(f, rows[0], board, 0);
    render_separator(f, rows[1]);
    render_row(f, rows[2], board, 3);
    render_separator(f, rows[3]);
    render_row(f, rows[4], board, 6);
}

fn render_row(f: &mut Frame, area: Rect, board: &Board, start: usize) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Length(1),
            Constraint::Percentage(33),
            Constraint::Length(1),
            Constraint::Percentage(34),
        ])
        .split(area);

    render_square(f, cols[0], board, start);
    render_vertical_sep(f, cols[1]);
    render_square(f, cols[2], board, start + 1);
    render_vertical_sep(f, cols[3]);
    render_square(f, cols[4], board, start + 2);
}

fn render_square(f: &mut Frame, area: Rect, board: &Board, pos: usize) {
    let square = board.get(pos).unwrap();
    let (text, style) = match square {
        Square::Empty => (
            format!("{}", pos + 1),
            Style::default().fg(Color::DarkGray),
        ),
        Square::Occupied(Player::X) => (
            "X".to_string(),
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
        ),
        Square::Occupied(Player::O) => (
            "O".to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    };
    let paragraph = Paragraph::new(text).style(style).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn render_separator(f: &mut Frame, area: Rect) {
    let sep = Paragraph::new("─".repeat(area.width as usize))
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(sep, area);
}

fn render_vertical_sep(f: &mut Frame, area: Rect) {
    let sep = Paragraph::new("│")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(sep, area);
}

fn center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Length((area.width.saturating_sub(width)) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((area.height.saturating_sub(height)) / 2),
        ])
        .split(horizontal[1])[1]
}
