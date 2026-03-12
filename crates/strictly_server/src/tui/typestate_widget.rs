//! Typestate graph visualization widget for the in-game TUI panel.
//!
//! Renders the game phase state machine as a box-and-arrow diagram using
//! Unicode box-drawing characters. The active phase is highlighted in cyan,
//! and a narrative callout drops below it showing the live situation and the
//! available transition choices.
//!
//! The event log beneath the graph tells the story of the hand in plain
//! language rather than technical proof names.

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
//  Phase context — narrative callout shown under the active node
// ─────────────────────────────────────────────────────────────

/// A single transition choice shown in the active-node callout.
#[derive(Debug, Clone)]
pub struct ChoiceHint {
    /// Key hint displayed in brackets, e.g. `"1"`.
    pub key: &'static str,
    /// Short action label, e.g. `"Hit"`.
    pub label: &'static str,
    /// Brief description of what the action does.
    pub desc: &'static str,
}

/// Narrative context for the currently active phase.
///
/// Populated by the game loop from live game state and passed to
/// [`TypestateGraphWidget`]. The widget renders it as a callout box
/// beneath the active node.
#[derive(Debug, Clone, Default)]
pub struct PhaseContext {
    /// One- or two-line plain-English description of the current situation.
    pub narrative: String,
    /// Available transitions the player can take right now.
    pub choices: Vec<ChoiceHint>,
}

impl PhaseContext {
    /// Create a context with a narrative and no discrete choices.
    pub fn info(narrative: impl Into<String>) -> Self {
        Self { narrative: narrative.into(), choices: Vec::new() }
    }

    /// Create a context with a narrative and choices.
    pub fn with_choices(
        narrative: impl Into<String>,
        choices: Vec<ChoiceHint>,
    ) -> Self {
        Self { narrative: narrative.into(), choices }
    }
}

// ─────────────────────────────────────────────────────────────
//  Game events for the story log
// ─────────────────────────────────────────────────────────────

/// A notable moment in the hand, shown in the story log panel.
#[derive(Debug, Clone)]
pub struct GameEvent {
    /// Display text — should read as plain English.
    pub text: String,
    /// Colour used when rendering.
    pub color: Color,
}

impl GameEvent {
    /// A story beat — free-form plain-English narrative.
    pub fn story(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: Color::White }
    }

    /// A phase transition, shown subtly so story beats stand out.
    pub fn phase_change(from: &str, to: &str) -> Self {
        Self {
            text: format!("  {} → {}", from, to),
            color: Color::DarkGray,
        }
    }

    /// A proof-carrying contract established (shown dimly — technical detail).
    pub fn proof(proof_name: &str) -> Self {
        Self {
            text: format!("  ✓ {}", proof_name),
            color: Color::DarkGray,
        }
    }

    /// Game concluded with a narrative outcome.
    pub fn result(text: impl Into<String>) -> Self {
        Self { text: text.into(), color: Color::Magenta }
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

/// Ratatui widget that renders the typestate graph, active callout, and story log.
///
/// The area is split vertically: upper portion is the phase graph with an
/// optional narrative callout beneath the active node; lower portion is the
/// story log showing the hand history in plain English.
#[derive(Debug)]
pub struct TypestateGraphWidget<'a> {
    /// Ordered list of phase nodes.
    pub nodes: &'a [NodeDef],
    /// Directed edges between nodes.
    pub edges: &'a [EdgeDef],
    /// Index of the currently active node (highlighted in cyan).
    pub active: Option<usize>,
    /// Recent game events shown in the story log (oldest first).
    pub events: &'a [GameEvent],
    /// Live phase context for the callout — `None` hides the callout.
    pub context: Option<&'a PhaseContext>,
}

impl<'a> TypestateGraphWidget<'a> {
    /// Creates a new widget without a phase callout.
    pub fn new(
        nodes: &'a [NodeDef],
        edges: &'a [EdgeDef],
        active: Option<usize>,
        events: &'a [GameEvent],
    ) -> Self {
        Self { nodes, edges, active, events, context: None }
    }

    /// Attaches a live phase context that renders as a callout under the active node.
    pub fn with_context(mut self, ctx: &'a PhaseContext) -> Self {
        self.context = Some(ctx);
        self
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

        // 60% of height for the graph+callout, rest for the story log.
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
    /// Renders the node row plus the optional narrative callout.
    fn render_graph(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 4 || self.nodes.is_empty() {
            return;
        }

        let n = self.nodes.len();
        let box_h: usize = 3;

        // Decide whether we have room for a callout.
        let callout_lines = self.callout_height();
        let needs_callout = callout_lines > 0;
        let connector_rows: usize = if needs_callout { 1 } else { 0 };
        let total_needed = box_h + connector_rows + callout_lines;

        // Position the node row: if callout fits, push nodes to top; else centre.
        let node_row_y = if needs_callout && total_needed <= area.height as usize {
            area.y
        } else {
            area.y + (area.height as usize).saturating_sub(box_h) as u16 / 2
        };

        // Box width = label length + 4 (borders + padding).
        let box_widths: Vec<usize> =
            self.nodes.iter().map(|nd| nd.label.len() + 4).collect();
        let total_w = area.width as usize;
        let slot_w = (total_w / n).max(1);

        // Horizontal centre of each box (for arrows and connector).
        let positions: Vec<(u16, u16)> = (0..n)
            .map(|i| {
                let slot_x = i * slot_w;
                let bw = box_widths[i];
                let bx_rel = slot_x + slot_w.saturating_sub(bw) / 2;
                (area.x + bx_rel as u16, node_row_y)
            })
            .collect();

        let arrow_style = Style::default().fg(Color::DarkGray);

        // ── Draw forward arrows ──────────────────────────────────
        for edge in self.edges {
            if edge.to == edge.from + 1 {
                let (bx_from, _) = positions[edge.from];
                let (bx_to, _) = positions[edge.to];
                let aw = box_widths[edge.from] as u16;
                let arrow_y = node_row_y + 1;
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

        // ── Draw back-edges ──────────────────────────────────────
        for edge in self.edges {
            if edge.to < edge.from {
                let arc_y = node_row_y + box_h as u16;
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
                if mid_to < area.x + area.width {
                    buf[(mid_to, arc_y)].set_char('◀').set_style(arrow_style);
                }
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

        // ── Draw node boxes ──────────────────────────────────────
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
            let top = format!("┌{}┐", "─".repeat((bw as usize).saturating_sub(2)));
            buf.set_string(bx, by, &top, border_style);
            let mid_y = by + 1;
            if mid_y < area.y + area.height {
                let interior = (bw as usize).saturating_sub(4);
                let padded = format!("{:^width$}", node.label, width = interior);
                let mid = format!("│ {} │", padded);
                buf.set_string(bx, mid_y, &mid, label_style);
            }
            let bot_y = by + 2;
            if bot_y < area.y + area.height {
                let bot = format!("└{}┘", "─".repeat((bw as usize).saturating_sub(2)));
                buf.set_string(bx, bot_y, &bot, border_style);
            }
        }

        // ── Draw callout ─────────────────────────────────────────
        if needs_callout {
            if let (Some(active_idx), Some(ctx)) = (self.active, self.context) {
                let (bx_active, _) = positions[active_idx];
                let bw_active = box_widths[active_idx] as u16;
                let connector_x = bx_active + bw_active / 2;
                let connector_y = node_row_y + box_h as u16;

                if connector_y < area.y + area.height {
                    buf[(connector_x, connector_y)]
                        .set_char('│')
                        .set_style(Style::default().fg(Color::Cyan));
                }

                let callout_y = connector_y + 1;
                if callout_y < area.y + area.height {
                    self.render_callout(ctx, area, callout_y, buf);
                }
            }
        }
    }

    /// Height (rows) required for the callout, or 0 if no callout needed.
    fn callout_height(&self) -> usize {
        match (self.active, self.context) {
            (Some(_), Some(ctx)) if !ctx.narrative.is_empty() => {
                // border top + narrative + blank + choices + border bottom
                2 + 1 + if ctx.choices.is_empty() { 0 } else { ctx.choices.len() + 1 }
            }
            _ => 0,
        }
    }

    /// Renders the callout box at `callout_y` spanning the full inner width.
    fn render_callout(&self, ctx: &PhaseContext, area: Rect, callout_y: u16, buf: &mut Buffer) {
        // Span from area.x to area.x + area.width - 1, max 48 chars wide.
        let max_w = (area.width as usize).min(48);
        let cw = max_w as u16;
        let cx = area.x;
        let inner_w = (cw as usize).saturating_sub(2); // inside the borders

        let border_style = Style::default().fg(Color::Cyan);
        let narrative_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let choice_key_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let choice_label_style = Style::default().fg(Color::Cyan);
        let choice_desc_style = Style::default().fg(Color::Gray);

        let mut row = callout_y;

        // Top border.
        if row < area.y + area.height {
            let top = format!("┌{}┐", "─".repeat(inner_w));
            buf.set_string(cx, row, &top, border_style);
            row += 1;
        }

        // Narrative line(s).
        if row < area.y + area.height {
            let clipped = clip_str(&ctx.narrative, inner_w);
            let padded = format!("│ {:<width$} │", clipped, width = inner_w.saturating_sub(2));
            buf.set_string(cx, row, &padded, narrative_style);
            row += 1;
        }

        // Choices.
        if !ctx.choices.is_empty() {
            // Blank separator.
            if row < area.y + area.height {
                let blank = format!("│{}│", " ".repeat(inner_w));
                buf.set_string(cx, row, &blank, border_style);
                row += 1;
            }

            for choice in &ctx.choices {
                if row >= area.y + area.height {
                    break;
                }
                // "│ [H] Hit      draw another card   │"
                let key_part = format!("[{}] ", choice.key);
                let label_part = format!("{:<8}", choice.label);
                let desc_part = clip_str(
                    choice.desc,
                    inner_w.saturating_sub(key_part.len() + label_part.len() + 2),
                );
                let _trailing =
                    " ".repeat(inner_w.saturating_sub(2 + key_part.len() + label_part.len() + desc_part.len()));

                // Write left border.
                buf[(cx, row)].set_char('│').set_style(border_style);
                buf[(cx + 1, row)].set_char(' ').set_style(border_style);
                let mut col = cx + 2;

                // [key]
                for (j, ch) in key_part.chars().enumerate() {
                    if col + j as u16 >= cx + cw { break; }
                    buf[(col + j as u16, row)].set_char(ch).set_style(choice_key_style);
                }
                col += key_part.len() as u16;

                // label
                for (j, ch) in label_part.chars().enumerate() {
                    if col + j as u16 >= cx + cw { break; }
                    buf[(col + j as u16, row)].set_char(ch).set_style(choice_label_style);
                }
                col += label_part.len() as u16;

                // desc
                for (j, ch) in desc_part.chars().enumerate() {
                    if col + j as u16 >= cx + cw { break; }
                    buf[(col + j as u16, row)].set_char(ch).set_style(choice_desc_style);
                }
                col += desc_part.len() as u16;

                // trailing spaces + right border
                let right_x = cx + cw - 1;
                for x in col..right_x {
                    if x < cx + cw { buf[(x, row)].set_char(' ').set_style(border_style); }
                }
                if right_x < cx + cw {
                    buf[(right_x, row)].set_char('│').set_style(border_style);
                }

                row += 1;
            }
        }

        // Bottom border.
        if row < area.y + area.height {
            let bot = format!("└{}┘", "─".repeat(inner_w));
            buf.set_string(cx, row, &bot, border_style);
        }
    }

    /// Renders the story log below the graph.
    fn render_log(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::TOP).title(" Story ");
        let inner = block.inner(area);
        block.render(area, buf);

        if self.events.is_empty() {
            Paragraph::new("Waiting for first hand…")
                .style(Style::default().fg(Color::DarkGray))
                .render(inner, buf);
            return;
        }

        let max_lines = inner.height as usize;
        let lines: Vec<Line> = self
            .events
            .iter()
            .rev()
            .take(max_lines)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
            .map(|(i, ev)| {
                // Most-recent entry is full brightness; older entries fade.
                let age = max_lines.saturating_sub(i + 1);
                let style = if age == 0 {
                    Style::default().fg(ev.color).add_modifier(Modifier::BOLD)
                } else if age < 3 {
                    Style::default().fg(ev.color)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(ev.text.clone(), style))
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}

/// Clips a string to at most `max_chars` characters.
fn clip_str(s: &str, max_chars: usize) -> &str {
    let mut end = 0;
    for (i, (byte_pos, _)) in s.char_indices().enumerate() {
        if i >= max_chars { break; }
        end = byte_pos + s[byte_pos..].chars().next().map_or(0, |c| c.len_utf8());
    }
    &s[..end]
}
