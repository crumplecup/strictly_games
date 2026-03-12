//! Typestate graph visualization widget for the in-game TUI panel.
//!
//! Renders the game phase state machine as a box-and-arrow diagram using
//! Unicode box-drawing characters. The active phase is highlighted in cyan.
//! Recent game events are shown in a log panel below the graph.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use tracing::instrument;

use crate::AnyGame;

// ─────────────────────────────────────────────────────────────
//  Graph definition types
// ─────────────────────────────────────────────────────────────

/// A node in the typestate graph.
#[derive(Debug, Clone)]
pub struct NodeDef {
    /// Display label rendered inside the box.
    pub label: &'static str,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone)]
pub struct EdgeDef {
    /// Index of the source node.
    pub from: usize,
    /// Index of the target node.
    pub to: usize,
}

// ─────────────────────────────────────────────────────────────
//  Game events for the log panel
// ─────────────────────────────────────────────────────────────

/// A notable event that occurred during gameplay, shown in the event log.
#[derive(Debug, Clone)]
pub struct GameEvent {
    /// Display text.
    pub text: String,
    /// Colour used when rendering.
    pub color: Color,
}

impl GameEvent {
    /// Phase transition event.
    pub fn phase_change(from: &'static str, to: &'static str) -> Self {
        Self {
            text: format!("→ {} (from {})", to, from),
            color: Color::Cyan,
        }
    }

    /// Proof-carrying type established event.
    pub fn proof(proof_name: &str) -> Self {
        Self {
            text: format!("✓ Established<{}>", proof_name),
            color: Color::Green,
        }
    }

    /// Game concluded with an outcome.
    pub fn result(outcome: &str) -> Self {
        Self {
            text: format!("Result: {}", outcome),
            color: Color::Magenta,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Blackjack graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the blackjack typestate graph (in display order).
pub fn blackjack_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "Betting"    },
        NodeDef { label: "PlayerTurn" },
        NodeDef { label: "DealerTurn" },
        NodeDef { label: "Finished"   },
    ]
}

/// Edge definitions for the blackjack typestate graph.
pub fn blackjack_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef { from: 0, to: 1 }, // place_bet → PlayerTurn
        EdgeDef { from: 1, to: 2 }, // stand → DealerTurn
        EdgeDef { from: 2, to: 3 }, // play_dealer_turn → Finished
    ]
}

/// Maps a blackjack phase name to the active node index.
#[instrument(level = "trace")]
pub fn blackjack_active(phase: &str) -> Option<usize> {
    match phase {
        "Betting"    => Some(0),
        "PlayerTurn" => Some(1),
        "DealerTurn" => Some(2),
        "Finished"   => Some(3),
        _            => None,
    }
}

// ─────────────────────────────────────────────────────────────
//  TicTacToe graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the tictactoe typestate graph (in display order).
pub fn tictactoe_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "GameSetup"    },
        NodeDef { label: "InProgress"   },
        NodeDef { label: "GameFinished" },
    ]
}

/// Edge definitions for the tictactoe typestate graph.
pub fn tictactoe_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef { from: 0, to: 1 }, // .start()
        EdgeDef { from: 1, to: 2 }, // .make_move() → terminal
        EdgeDef { from: 2, to: 0 }, // .restart()
    ]
}

/// Maps the current `AnyGame` to the active node index in the tictactoe graph.
#[instrument(skip(game))]
pub fn tictactoe_active(game: &AnyGame) -> Option<usize> {
    match game {
        AnyGame::Setup { .. }                                                       => Some(0),
        AnyGame::InProgress { .. }                                                  => Some(1),
        AnyGame::Won { .. } | AnyGame::Draw { .. } | AnyGame::Finished { .. }      => Some(2),
    }
}

/// Returns the phase name string for an `AnyGame` (used for event logging).
pub fn tictactoe_phase_name(game: &AnyGame) -> &'static str {
    match game {
        AnyGame::Setup { .. }      => "Setup",
        AnyGame::InProgress { .. } => "InProgress",
        _                          => "Finished",
    }
}

// ─────────────────────────────────────────────────────────────
//  Widget
// ─────────────────────────────────────────────────────────────

/// Ratatui widget that renders the typestate graph and event log.
///
/// The area is split vertically: the upper portion shows the phase graph as
/// box-and-arrow ASCII art, and the lower portion shows a scrolling event log.
#[derive(Debug)]
pub struct TypestateGraphWidget<'a> {
    /// Ordered list of phase nodes.
    pub nodes: &'a [NodeDef],
    /// Directed edges between nodes.
    pub edges: &'a [EdgeDef],
    /// Index of the currently active node (highlighted in cyan).
    pub active: Option<usize>,
    /// Recent game events shown in the log panel (oldest first).
    pub events: &'a [GameEvent],
}

impl<'a> TypestateGraphWidget<'a> {
    /// Creates a new widget.
    pub fn new(
        nodes: &'a [NodeDef],
        edges: &'a [EdgeDef],
        active: Option<usize>,
        events: &'a [GameEvent],
    ) -> Self {
        Self { nodes, edges, active, events }
    }
}

impl Widget for TypestateGraphWidget<'_> {
    #[instrument(skip_all)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .title(" Typestate ");
        let inner = outer.inner(area);
        outer.render(area, buf);

        if inner.height < 5 || inner.width < 10 || self.nodes.is_empty() {
            return;
        }

        // 60 % of height for the graph, rest for the event log.
        let graph_h = ((inner.height as u32 * 6 / 10) as u16).max(5);
        let log_h = inner.height.saturating_sub(graph_h);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(graph_h), Constraint::Length(log_h)])
            .split(inner);

        self.render_graph(chunks[0], buf);
        self.render_log(chunks[1], buf);
    }
}

impl TypestateGraphWidget<'_> {
    /// Renders the box-and-arrow phase diagram into `area`.
    fn render_graph(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 4 || self.nodes.is_empty() {
            return;
        }

        let n = self.nodes.len();

        // Box width = label length + 2 side borders + 2 spaces of padding.
        let box_widths: Vec<usize> = self.nodes
            .iter()
            .map(|node| node.label.len() + 4)
            .collect();

        // Divide available width into equal slots, one per node.
        let total_w = area.width as usize;
        let slot_w = (total_w / n).max(1);
        let box_h: usize = 3;

        // Centre boxes vertically in the graph area.
        let box_y_rel = (area.height as usize).saturating_sub(box_h) / 2;

        // Absolute (x, y) top-left corner for each box.
        let positions: Vec<(u16, u16)> = (0..n)
            .map(|i| {
                let slot_x = i * slot_w;
                let bw = box_widths[i];
                let bx_rel = slot_x + slot_w.saturating_sub(bw) / 2;
                (area.x + bx_rel as u16, area.y + box_y_rel as u16)
            })
            .collect();

        let arrow_style = Style::default().fg(Color::DarkGray);

        // Draw forward arrows first so boxes render on top.
        for edge in self.edges {
            if edge.to == edge.from + 1 {
                let (bx_from, _) = positions[edge.from];
                let (bx_to, _) = positions[edge.to];
                let aw = box_widths[edge.from] as u16;
                let arrow_y = positions[edge.from].1 + 1;
                let x_start = bx_from + aw;
                let x_end = bx_to.saturating_sub(1);

                for x in x_start..x_end {
                    if x < area.x + area.width {
                        buf[(x, arrow_y)].set_char('─').set_style(arrow_style);
                    }
                }
                if x_end < area.x + area.width && x_end >= x_start {
                    buf[(x_end, arrow_y)].set_char('▶').set_style(arrow_style);
                }
            }
        }

        // Draw return arcs (back-edges) as a line below the boxes.
        for edge in self.edges {
            if edge.to < edge.from {
                let arc_y = positions[edge.from].1 + box_h as u16;
                if arc_y >= area.y + area.height {
                    continue;
                }

                let (bx_from, _) = positions[edge.from];
                let (bx_to, _) = positions[edge.to];
                let mid_from = bx_from + box_widths[edge.from] as u16 / 2;
                let mid_to = bx_to + box_widths[edge.to] as u16 / 2;

                for x in mid_to..=mid_from {
                    if x < area.x + area.width {
                        buf[(x, arc_y)].set_char('─').set_style(arrow_style);
                    }
                }
                // Arrowhead pointing left toward the destination node.
                if mid_to < area.x + area.width {
                    buf[(mid_to, arc_y)].set_char('◀').set_style(arrow_style);
                }
                // Vertical connectors from each box bottom down to the arc.
                let vert_y = arc_y.saturating_sub(1);
                if vert_y < area.y + area.height {
                    if mid_to < area.x + area.width {
                        buf[(mid_to, vert_y)].set_char('│').set_style(arrow_style);
                    }
                    if mid_from < area.x + area.width {
                        buf[(mid_from, vert_y)].set_char('│').set_style(arrow_style);
                    }
                }
            }
        }

        // Draw node boxes.
        for (i, node) in self.nodes.iter().enumerate() {
            let is_active = self.active == Some(i);
            let (bx, by) = positions[i];
            let bw = box_widths[i] as u16;

            let (border_style, label_style) = if is_active {
                (
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    Style::default().fg(Color::DarkGray),
                    Style::default().fg(Color::Gray),
                )
            };

            if bx >= area.x + area.width || by >= area.y + area.height {
                continue;
            }

            // Top border: ┌───┐
            let top = format!("┌{}┐", "─".repeat((bw as usize).saturating_sub(2)));
            buf.set_string(bx, by, &top, border_style);

            // Middle row: │ label │
            let mid_y = by + 1;
            if mid_y < area.y + area.height {
                let interior = (bw as usize).saturating_sub(4);
                let padded = format!("{:^width$}", node.label, width = interior);
                let mid = format!("│ {} │", padded);
                buf.set_string(bx, mid_y, &mid, label_style);
            }

            // Bottom border: └───┘
            let bot_y = by + 2;
            if bot_y < area.y + area.height {
                let bot = format!("└{}┘", "─".repeat((bw as usize).saturating_sub(2)));
                buf.set_string(bx, bot_y, &bot, border_style);
            }
        }
    }

    /// Renders the event log below the graph.
    fn render_log(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::TOP).title(" Events ");
        let inner = block.inner(area);
        block.render(area, buf);

        if self.events.is_empty() {
            Paragraph::new("No events yet.")
                .style(Style::default().fg(Color::DarkGray))
                .render(inner, buf);
            return;
        }

        // Show the most recent N events (oldest first, most recent at bottom).
        let lines: Vec<Line> = self
            .events
            .iter()
            .rev()
            .take(inner.height as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|ev| Line::from(Span::styled(ev.text.clone(), Style::default().fg(ev.color))))
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
