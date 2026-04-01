//! Typestate graph visualization widget for the in-game TUI panel.
//!
//! Renders the game phase state machine as a box-and-arrow diagram using
//! composed `Block`, `Paragraph`, `Line`, and `Span` widgets — no direct
//! buffer manipulation. This makes the entire output expressible as
//! `WidgetJson` for AccessKit verification.
//!
//! The active phase is highlighted in cyan, and a narrative callout drops
//! below it showing the live situation and the available transition choices.
//!
//! The event log beneath the graph tells the story of the hand in plain
//! language rather than technical proof names.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use tracing::instrument;
use unicode_width::UnicodeWidthStr as _;

use crate::AnyGame;

// ─────────────────────────────────────────────────────────────
//  Unicode-aware text helpers
// ─────────────────────────────────────────────────────────────

/// Truncates `s` so its display width does not exceed `max_cols`.
///
/// If truncation is required, the last visible character is replaced with `…`
/// (U+2026, display width 1), so the result always fits in `max_cols` columns.
fn truncate_to_width(s: &str, max_cols: usize) -> String {
    if s.width() <= max_cols {
        return s.to_string();
    }
    if max_cols == 0 {
        return String::new();
    }
    if max_cols == 1 {
        return "…".to_string();
    }
    // Reserve one column for the ellipsis.
    let budget = max_cols - 1;
    let mut used = 0;
    let mut end = 0;
    for ch in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        end += ch.len_utf8();
    }
    format!("{}…", &s[..end])
}

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
    /// Optional short label rendered at the midpoint of the arc.
    ///
    /// Used on skip-forward edges (bypass paths) to name the transition.
    pub label: Option<&'static str>,
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
    /// The exact prompt text currently in-flight to the agent, captured by
    /// [`ObservableCommunicator`] the moment before `send_prompt` blocks.
    ///
    /// `None` when no elicitation is active. When `Some`, the widget renders
    /// the full assembled prompt (options list included) in a highlighted panel
    /// so the observer can see precisely what the agent was asked.
    pub pending_prompt: Option<String>,
}

impl PhaseContext {
    /// Create a context with a narrative and no discrete choices.
    pub fn info(narrative: impl Into<String>) -> Self {
        Self {
            narrative: narrative.into(),
            choices: Vec::new(),
            pending_prompt: None,
        }
    }

    /// Create a context with a narrative and choices.
    pub fn with_choices(narrative: impl Into<String>, choices: Vec<ChoiceHint>) -> Self {
        Self {
            narrative: narrative.into(),
            choices,
            pending_prompt: None,
        }
    }

    /// Attach a live in-flight prompt snapshot from [`ObservableCommunicator`].
    pub fn with_pending_prompt(mut self, prompt: Option<String>) -> Self {
        self.pending_prompt = prompt;
        self
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
        Self {
            text: text.into(),
            color: Color::White,
        }
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
        Self {
            text: text.into(),
            color: Color::Magenta,
        }
    }
}

// Re-export ExploreStats from session for use in the widget
pub use crate::session::ExploreStats;

// ─────────────────────────────────────────────────────────────
//  Blackjack graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the blackjack typestate graph (in display order).
pub fn blackjack_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "Betting" },
        NodeDef {
            label: "PlayerTurn",
        },
        NodeDef {
            label: "DealerTurn",
        },
        NodeDef { label: "Finished" },
    ]
}

/// Edge definitions for the blackjack typestate graph.
pub fn blackjack_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: None,
        },
        EdgeDef {
            from: 0,
            to: 3,
            label: Some("(natural)"),
        },
    ]
}

/// Maps a blackjack phase name to the active node index.
#[instrument(level = "trace")]
pub fn blackjack_active(phase: &str) -> Option<usize> {
    match phase {
        "Betting" => Some(0),
        "PlayerTurn" => Some(1),
        "DealerTurn" => Some(2),
        "Finished" => Some(3),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────
//  TicTacToe graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the tictactoe typestate graph (in display order).
///
/// The `InProgress` phase is split into `X Turn` and `O Turn` sub-nodes
/// so the graph reflects whose move it is.
pub fn tictactoe_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "GameSetup" },
        NodeDef { label: "X Turn" },
        NodeDef { label: "O Turn" },
        NodeDef {
            label: "GameFinished",
        },
    ]
}

/// Edge definitions for the tictactoe typestate graph.
pub fn tictactoe_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 3,
            label: Some("(end)"),
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: Some("(end)"),
        },
    ]
}

/// Maps the current `AnyGame` to the active node index in the tictactoe graph.
#[instrument(skip(game))]
pub fn tictactoe_active(game: &AnyGame) -> Option<usize> {
    use crate::games::tictactoe::Player;

    match game {
        AnyGame::Setup { .. } => Some(0),
        AnyGame::InProgress { .. } => match game.to_move() {
            Some(Player::X) => Some(1),
            Some(Player::O) => Some(2),
            None => Some(1),
        },
        AnyGame::Won { .. } | AnyGame::Draw { .. } | AnyGame::Finished { .. } => Some(3),
    }
}

/// Returns the phase name string for an `AnyGame` (used for event logging).
pub fn tictactoe_phase_name(game: &AnyGame) -> &'static str {
    use crate::games::tictactoe::Player;

    match game {
        AnyGame::Setup { .. } => "Setup",
        AnyGame::InProgress { .. } => match game.to_move() {
            Some(Player::X) => "X Turn",
            Some(Player::O) => "O Turn",
            None => "InProgress",
        },
        _ => "Finished",
    }
}

// ─────────────────────────────────────────────────────────────
//  Craps graph definition
// ─────────────────────────────────────────────────────────────

/// Node definitions for the craps typestate graph (in display order).
pub fn craps_nodes() -> Vec<NodeDef> {
    vec![
        NodeDef { label: "Betting" },
        NodeDef { label: "ComeOut" },
        NodeDef {
            label: "PointPhase",
        },
        NodeDef { label: "Resolved" },
    ]
}

/// Edge definitions for the craps typestate graph.
pub fn craps_edges() -> Vec<EdgeDef> {
    vec![
        EdgeDef {
            from: 0,
            to: 1,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 2,
            label: None,
        },
        EdgeDef {
            from: 2,
            to: 3,
            label: None,
        },
        EdgeDef {
            from: 1,
            to: 3,
            label: Some("(natural/craps)"),
        },
        EdgeDef {
            from: 3,
            to: 0,
            label: Some("(next round)"),
        },
    ]
}

/// Maps a craps phase name to the active node index.
#[instrument(level = "trace")]
pub fn craps_active(phase: &str) -> Option<usize> {
    match phase {
        "Betting" => Some(0),
        "ComeOut" => Some(1),
        "PointPhase" => Some(2),
        "Resolved" => Some(3),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────
//  Widget
// ─────────────────────────────────────────────────────────────

/// Ratatui widget that renders the typestate graph, active callout, and story log.
///
/// All rendering uses composed `Block`, `Paragraph`, `Line`, and `Span` widgets.
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
    /// Agent explore/play stats — `None` hides the stats bar.
    pub explore_stats: Option<&'a ExploreStats>,
}

impl<'a> TypestateGraphWidget<'a> {
    /// Creates a new widget without a phase callout.
    pub fn new(
        nodes: &'a [NodeDef],
        edges: &'a [EdgeDef],
        active: Option<usize>,
        events: &'a [GameEvent],
    ) -> Self {
        Self {
            nodes,
            edges,
            active,
            events,
            context: None,
            explore_stats: None,
        }
    }

    /// Attaches a live phase context that renders as a callout under the active node.
    pub fn with_context(mut self, ctx: &'a PhaseContext) -> Self {
        self.context = Some(ctx);
        self
    }

    /// Attaches explore/play tracking stats.
    pub fn with_explore_stats(mut self, stats: &'a ExploreStats) -> Self {
        self.explore_stats = Some(stats);
        self
    }
}

impl Widget for TypestateGraphWidget<'_> {
    #[instrument(skip_all)]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let outer = Block::default().borders(Borders::ALL).title(" Typestate ");
        let inner = outer.inner(area);
        outer.render(area, buf);

        if inner.height < 5 || inner.width < 10 || self.nodes.is_empty() {
            return;
        }

        let has_callout = self.has_callout();
        let has_stats = self
            .explore_stats
            .is_some_and(|s| s.total_explores > 0 || s.total_plays > 0);

        // Node row (3 lines) + arrow row (1) + optional callout + optional stats
        let node_row_h: u16 = 3;
        let arrow_row_h: u16 = 1;
        let callout_h: u16 = if has_callout {
            self.callout_line_count() as u16 + 2 // +2 for Block borders
        } else {
            0
        };
        let connector_h: u16 = if has_callout { 1 } else { 0 };
        let stats_h: u16 = if has_stats { 1 } else { 0 };
        let graph_total =
            (node_row_h + arrow_row_h + connector_h + callout_h + stats_h).min(inner.height);
        let log_h = inner.height.saturating_sub(graph_total);

        let mut constraints: Vec<Constraint> = vec![
            Constraint::Length(node_row_h),
            Constraint::Length(arrow_row_h),
        ];
        if has_callout {
            constraints.push(Constraint::Length(connector_h));
            constraints.push(Constraint::Length(callout_h));
        }
        if has_stats {
            constraints.push(Constraint::Length(stats_h));
        }
        constraints.push(Constraint::Length(log_h));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut idx = 0;
        self.render_node_row(chunks[idx], buf);
        idx += 1;
        self.render_arrow_row(chunks[idx], buf);
        idx += 1;
        if has_callout {
            self.render_connector(chunks[idx], buf);
            idx += 1;
            self.render_callout_block(chunks[idx], buf);
            idx += 1;
        }
        if has_stats {
            self.render_explore_stats(chunks[idx], buf);
            idx += 1;
        }
        self.render_log(chunks[idx], buf);
    }
}

impl TypestateGraphWidget<'_> {
    /// Builds the node row: a horizontal layout of `Block` widgets with centered labels.
    ///
    /// Each node is rendered as a bordered `Block` containing a centered `Paragraph`.
    /// Active node gets cyan border + bold inverted label.
    #[instrument(skip_all)]
    fn render_node_row(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 || self.nodes.is_empty() {
            return;
        }

        let n = self.nodes.len();
        // Interleave node slots with arrow gap slots
        let mut constraints: Vec<Constraint> = Vec::with_capacity(n * 2 - 1);
        for i in 0..n {
            constraints.push(Constraint::Length(
                (self.nodes[i].label.width() as u16 + 4).min(area.width),
            ));
            if i < n - 1 {
                constraints.push(Constraint::Min(1)); // arrow gap
            }
        }

        let slots = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(area);

        for (i, node) in self.nodes.iter().enumerate() {
            let slot_idx = i * 2;
            let is_active = self.active == Some(i);

            let (border_style, label_style) = if is_active {
                (
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
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

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style);
            let node_inner = block.inner(slots[slot_idx]);
            let truncated = truncate_to_width(node.label, node_inner.width as usize);
            let label = Paragraph::new(Line::from(Span::styled(truncated, label_style)))
                .alignment(Alignment::Center);
            block.render(slots[slot_idx], buf);
            label.render(node_inner, buf);

            // Forward arrow in the gap between consecutive nodes
            if i < n - 1 {
                let gap = slots[slot_idx + 1];
                if gap.width > 0 && gap.height > 0 {
                    let has_forward = self.edges.iter().any(|e| e.from == i && e.to == i + 1);
                    if has_forward {
                        let arrow_style = Style::default().fg(Color::DarkGray);
                        let arrow_chars: String = if gap.width <= 1 {
                            "▶".to_string()
                        } else {
                            let dashes = "─".repeat((gap.width as usize).saturating_sub(1));
                            format!("{dashes}▶")
                        };
                        // Render the arrow on the middle row of the gap
                        let arrow_y = gap.y + gap.height.saturating_sub(1) / 2;
                        let arrow_line = Line::from(Span::styled(arrow_chars, arrow_style));
                        let arrow_area = Rect::new(gap.x, arrow_y, gap.width, 1);
                        Paragraph::new(arrow_line).render(arrow_area, buf);
                    }
                }
            }
        }
    }

    /// Renders the arc row below nodes for back-edges and skip-forward edges.
    ///
    /// Back-edges (to < from) rendered with `◀───` in dark gray.
    /// Skip-forward edges (to > from + 1) rendered with `───▶` in yellow with labels.
    #[instrument(skip_all)]
    fn render_arrow_row(&self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height == 0 {
            return;
        }

        let n = self.nodes.len();
        let slot_w = (area.width as usize / n).max(1);

        // Compute horizontal midpoints for each node slot
        let midpoints: Vec<u16> = (0..n)
            .map(|i| {
                let box_w = self.nodes[i].label.width() + 4;
                let slot_x = i * slot_w;
                let bx = slot_x + slot_w.saturating_sub(box_w) / 2;
                area.x + (bx + box_w / 2) as u16
            })
            .collect();

        // Build the arc row as a line of Spans
        let arc_spans = build_arc_spans(self.edges, &midpoints, area.x, area.width);

        if !arc_spans.is_empty() {
            let line = Line::from(arc_spans);
            Paragraph::new(line).render(area, buf);
        }
    }

    /// Renders the `│` connector between the active node and callout.
    #[instrument(skip_all)]
    fn render_connector(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // Place the connector at the horizontal center of the active node
        if let Some(active_idx) = self.active {
            let n = self.nodes.len();
            let slot_w = (area.width as usize / n).max(1);
            let box_w = self.nodes[active_idx].label.width() + 4;
            let slot_x = active_idx * slot_w;
            let bx = slot_x + slot_w.saturating_sub(box_w) / 2;
            let mid = area.x + (bx + box_w / 2) as u16;

            let connector_style = Style::default().fg(Color::Cyan);
            // Build a line with spaces up to mid, then │
            let offset = (mid.saturating_sub(area.x)) as usize;
            let pad = " ".repeat(offset);
            let line = Line::from(vec![Span::raw(pad), Span::styled("│", connector_style)]);
            Paragraph::new(line).render(area, buf);
        }
    }

    /// Whether we should show a callout.
    fn has_callout(&self) -> bool {
        matches!(
            (self.active, self.context),
            (Some(_), Some(ctx)) if !ctx.narrative.is_empty()
        )
    }

    /// Number of content lines inside the callout (excluding block borders).
    fn callout_line_count(&self) -> usize {
        match (self.active, self.context) {
            (Some(_), Some(ctx)) if !ctx.narrative.is_empty() => {
                let prompt_rows = match &ctx.pending_prompt {
                    Some(p) => p.lines().count() + 2, // header + lines + separator
                    None => 0,
                };
                1 + if ctx.choices.is_empty() {
                    0
                } else {
                    ctx.choices.len() + 1
                } + prompt_rows
            }
            _ => 0,
        }
    }

    /// Renders the callout as a bordered `Block` with composed `Paragraph` content.
    #[instrument(skip_all)]
    fn render_callout_block(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 6 {
            return;
        }
        let Some(ctx) = self.context else { return };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line<'static>> = Vec::new();
        let inner_w = inner.width as usize;

        // Narrative
        let narrative_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let clipped = truncate_to_width(&ctx.narrative, inner_w);
        lines.push(Line::from(Span::styled(clipped, narrative_style)));

        // Choices
        if !ctx.choices.is_empty() {
            lines.push(Line::from(""));
            for choice in &ctx.choices {
                let key_style = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                let label_style = Style::default().fg(Color::Cyan);
                let desc_style = Style::default().fg(Color::Gray);

                let truncated_label = truncate_to_width(choice.label, 8);
                lines.push(Line::from(vec![
                    Span::styled(format!("[{}] ", choice.key), key_style),
                    Span::styled(format!("{:<8}", truncated_label), label_style),
                    Span::styled(choice.desc.to_string(), desc_style),
                ]));
            }
        }

        // Pending prompt
        if let Some(prompt_text) = &ctx.pending_prompt {
            let header_style = Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD);
            let text_style = Style::default().fg(Color::Magenta);

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "▶ Prompt in-flight:",
                header_style,
            )));
            for prompt_line in prompt_text.lines() {
                let clipped = truncate_to_width(prompt_line, inner_w);
                lines.push(Line::from(Span::styled(clipped, text_style)));
            }
        }

        Paragraph::new(lines).render(inner, buf);
    }

    /// Renders the explore/play stats bar between graph and story log.
    ///
    /// Shows a compact line like: `🔍 3 explores / 2 plays (1 this turn)`
    /// The bar turns yellow when per-turn explores exceed 3 (potential whirlpool).
    #[instrument(skip_all)]
    fn render_explore_stats(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 4 {
            return;
        }
        if let Some(stats) = self.explore_stats {
            let text = stats.status_line();
            let whirlpool = stats.turn_explores > 3;
            let style = if whirlpool {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };
            let suffix = if whirlpool { " ⚠ whirlpool?" } else { "" };
            let full = format!("{text}{suffix}");
            let line = Line::from(Span::styled(full, style));
            Paragraph::new(line).render(area, buf);
        }
    }

    /// Renders the story log below the graph.
    #[instrument(skip_all)]
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

// ─────────────────────────────────────────────────────────────
//  Arc span builder — constructs a styled Line for back/skip edges
// ─────────────────────────────────────────────────────────────

/// Builds styled spans for the arc row showing back-edges and skip-forward edges.
///
/// Each edge type gets its own character range on the row:
/// - Back-edges (to < from): `◀───` in dark gray
/// - Skip-forward edges (to > from + 1): `───▶` in yellow, with optional label
#[instrument(skip_all)]
fn build_arc_spans(
    edges: &[EdgeDef],
    midpoints: &[u16],
    area_x: u16,
    area_width: u16,
) -> Vec<Span<'static>> {
    let total_w = area_width as usize;
    if total_w == 0 {
        return Vec::new();
    }

    // Build a character + style buffer
    let mut chars: Vec<char> = vec![' '; total_w];
    let mut styles: Vec<Style> = vec![Style::default(); total_w];

    let arrow_style = Style::default().fg(Color::DarkGray);
    let skip_style = Style::default().fg(Color::Yellow);

    for edge in edges {
        if edge.from >= midpoints.len() || edge.to >= midpoints.len() {
            continue;
        }

        // Back-edge: to < from
        if edge.to < edge.from {
            let mid_to = midpoints[edge.to].saturating_sub(area_x) as usize;
            let mid_from = midpoints[edge.from].saturating_sub(area_x) as usize;
            if mid_to < total_w {
                chars[mid_to] = '◀';
                styles[mid_to] = arrow_style;
            }
            for x in (mid_to + 1)..mid_from.min(total_w) {
                chars[x] = '─';
                styles[x] = arrow_style;
            }
        }

        // Skip-forward edge: to > from + 1
        if edge.to > edge.from + 1 {
            let mid_from = midpoints[edge.from].saturating_sub(area_x) as usize;
            let mid_to = midpoints[edge.to].saturating_sub(area_x) as usize;
            for x in mid_from..mid_to.min(total_w) {
                chars[x] = '─';
                styles[x] = skip_style;
            }
            if mid_to < total_w {
                chars[mid_to] = '▶';
                styles[mid_to] = skip_style;
            }
            // Place label at midpoint
            if let Some(lbl) = edge.label {
                let mid_x = (mid_from + mid_to) / 2;
                let lbl_start = mid_x.saturating_sub(lbl.len() / 2);
                for (j, ch) in lbl.chars().enumerate() {
                    let x = lbl_start + j;
                    if x < total_w {
                        chars[x] = ch;
                        styles[x] = skip_style;
                    }
                }
            }
        }
    }

    // Collapse runs of same-styled characters into Spans
    if chars.iter().all(|c| *c == ' ') {
        return Vec::new();
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_start = 0;
    while run_start < total_w {
        let run_style = styles[run_start];
        let mut run_end = run_start + 1;
        while run_end < total_w && styles[run_end] == run_style {
            run_end += 1;
        }
        let text: String = chars[run_start..run_end].iter().collect();
        spans.push(Span::styled(text, run_style));
        run_start = run_end;
    }

    spans
}
