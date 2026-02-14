//! Stateless UI rendering for tic-tac-toe.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::games::tictactoe::{types::Board, Player, Position};

/// Renders the game board with cursor highlight.
pub fn draw(frame: &mut Frame, board: &Board, cursor: Position, status: &str) {
    let area = frame.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // Title
            Constraint::Min(9),      // Board
            Constraint::Length(3),   // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new("Strictly Games - Tic Tac Toe")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Board
    draw_board(frame, chunks[1], board, cursor);

    // Status
    let status_text = Paragraph::new(status)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(status_text, chunks[2]);
}

fn draw_board(frame: &mut Frame, area: Rect, board: &Board, cursor: Position) {
    // Center the board
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

    draw_row(frame, rows[0], board, cursor, &[Position::TopLeft, Position::TopCenter, Position::TopRight]);
    draw_separator(frame, rows[1]);
    draw_row(frame, rows[2], board, cursor, &[Position::MiddleLeft, Position::Center, Position::MiddleRight]);
    draw_separator(frame, rows[3]);
    draw_row(frame, rows[4], board, cursor, &[Position::BottomLeft, Position::BottomCenter, Position::BottomRight]);
}

fn draw_row(frame: &mut Frame, area: Rect, board: &Board, cursor: Position, positions: &[Position; 3]) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12),
            Constraint::Length(1),
            Constraint::Length(12),
            Constraint::Length(1),
            Constraint::Length(12),
        ])
        .split(area);

    draw_cell(frame, cols[0], board, cursor, positions[0]);
    draw_separator_vertical(frame, cols[1]);
    draw_cell(frame, cols[2], board, cursor, positions[1]);
    draw_separator_vertical(frame, cols[3]);
    draw_cell(frame, cols[4], board, cursor, positions[2]);
}

fn draw_cell(frame: &mut Frame, area: Rect, board: &Board, cursor: Position, pos: Position) {
    use crate::games::tictactoe::types::Square;

    let square = board.get(pos);
    
    let (symbol, base_style) = match square {
        Square::Empty => ("   ", Style::default().fg(Color::DarkGray)),
        Square::Occupied(Player::X) => (" X ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
        Square::Occupied(Player::O) => (" O ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
    };

    let style = if pos == cursor {
        base_style.bg(Color::White).fg(Color::Black)
    } else {
        base_style
    };

    let paragraph = Paragraph::new(Line::from(Span::styled(symbol, style)))
        .alignment(Alignment::Center);
    
    frame.render_widget(paragraph, area);
}

fn draw_separator(frame: &mut Frame, area: Rect) {
    let sep = Paragraph::new("─────────────────────────────────────────")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, area);
}

fn draw_separator_vertical(frame: &mut Frame, area: Rect) {
    let sep = Paragraph::new("│")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, area);
}

fn center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((area.height.saturating_sub(height)) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Length((area.width.saturating_sub(width)) / 2),
        ])
        .split(vert[1])[1]
}
