//! Chat widget for rendering the elicitation exchange between the host and players.
//!
//! Displays a scrolling chat log in a bordered ratatui panel, newest messages at
//! the bottom. All rendering uses composed `Paragraph`/`Line`/`Span` widgets —
//! no direct buffer manipulation — so the output is expressible as `WidgetJson`
//! for AccessKit verification.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use tokio::sync::mpsc;
use tracing::{debug, instrument};

use crate::tui::contracts::ChatWrapped;
use elicitation::contracts::Established;

// ─────────────────────────────────────────────────────────────
//  Participant
// ─────────────────────────────────────────────────────────────

/// Who sent a chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Participant {
    /// The game host (elicitation prompts).
    Host,
    /// The human player at the keyboard.
    Human,
    /// An AI agent player, identified by name.
    Agent(String),
}

impl Participant {
    /// Returns the display name for this participant.
    ///
    /// `Host` → `"Host"`, `Human` → `"You"`, `Agent(name)` → the agent's name.
    #[instrument]
    pub fn display_name(&self) -> &str {
        match self {
            Participant::Host => "Host",
            Participant::Human => "You",
            Participant::Agent(name) => name.as_str(),
        }
    }

    /// Returns the ratatui color associated with this participant.
    ///
    /// `Host` → cyan, `Human` → yellow, `Agent(_)` → green.
    #[instrument]
    pub fn color(&self) -> Color {
        match self {
            Participant::Host => Color::Cyan,
            Participant::Human => Color::Yellow,
            Participant::Agent(_) => Color::Green,
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  ChatMessage
// ─────────────────────────────────────────────────────────────

/// A single message in the chat log.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Who sent this message.
    pub participant: Participant,
    /// The message text.
    pub text: String,
}

impl ChatMessage {
    /// Creates a new [`ChatMessage`].
    #[instrument(skip(text))]
    pub fn new(participant: Participant, text: impl Into<String>) -> Self {
        let text = text.into();
        debug!(participant = ?participant, text_len = text.len(), "Creating ChatMessage");
        Self { participant, text }
    }

    /// Converts this message into a styled [`Line`] for rendering.
    ///
    /// Returns a single `Line` containing the participant prefix and full
    /// message text.  Long lines are word-wrapped by ratatui's `Wrap` on the
    /// enclosing `Paragraph` — no clipping occurs here.
    #[instrument(skip(self))]
    fn to_line(&self) -> Line<'static> {
        let style = Style::default().fg(self.participant.color());
        let prefix = format!("[{}] ", self.participant.display_name());
        Line::from(vec![
            Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
            Span::styled(self.text.clone(), style),
        ])
    }
}

// ─────────────────────────────────────────────────────────────
//  ChatWidget
// ─────────────────────────────────────────────────────────────

/// Renders the elicitation chat log as a composed `Paragraph` widget.
///
/// Displays messages newest-at-bottom within a bordered panel, colour-coded by
/// participant. Host messages are left-aligned; human and agent replies are
/// right-aligned via leading space padding in `Span`s.
pub struct ChatWidget<'a> {
    messages: &'a [ChatMessage],
    title: &'static str,
}

impl<'a> ChatWidget<'a> {
    /// Creates a new [`ChatWidget`], returning a `ChatWrapped` proof token.
    ///
    /// The proof is established by construction: this widget always enables
    /// ratatui word-wrap on the inner `Paragraph`, so text never overflows.
    #[instrument(skip(messages))]
    pub fn new(messages: &'a [ChatMessage]) -> (Self, Established<ChatWrapped>) {
        debug!(message_count = messages.len(), "Creating ChatWidget");
        let widget = Self {
            messages,
            title: " 💬 Chat ",
        };
        (widget, Established::assert())
    }
}

// ─────────────────────────────────────────────────────────────
//  Widget impl — composed from Paragraph/Line/Span
// ─────────────────────────────────────────────────────────────

impl Widget for ChatWidget<'_> {
    #[instrument(skip(self, buf))]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).title(self.title);
        let inner = block.inner(area);

        if inner.height == 0 || inner.width == 0 {
            block.render(area, buf);
            return;
        }

        let lines: Vec<Line<'static>> = if self.messages.is_empty() {
            debug!("No messages; rendering placeholder");
            vec![Line::from(Span::styled(
                "Waiting for first exchange…",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            debug!(total = self.messages.len(), "Rendering chat messages");
            self.messages.iter().map(|msg| msg.to_line()).collect()
        };

        // Word-wrap is always enabled — the ChatWrapped contract is proved by
        // the fact that this Paragraph always carries Wrap { trim: false }.
        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        paragraph.render(area, buf);
    }
}

// ─────────────────────────────────────────────────────────────
//  Channel helper
// ─────────────────────────────────────────────────────────────

/// Creates an unbounded chat channel.
///
/// Returns `(sender, receiver)` for distributing to communicators and the render loop.
#[instrument]
pub fn chat_channel() -> (
    mpsc::UnboundedSender<ChatMessage>,
    mpsc::UnboundedReceiver<ChatMessage>,
) {
    debug!("Creating unbounded chat channel");
    mpsc::unbounded_channel()
}
