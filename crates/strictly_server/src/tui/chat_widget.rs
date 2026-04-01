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
    widgets::{Block, Borders, Paragraph, Widget},
};
use tokio::sync::mpsc;
use tracing::{debug, instrument};

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

    /// Returns `true` if this participant is the game host.
    #[instrument]
    pub fn is_host(&self) -> bool {
        matches!(self, Participant::Host)
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
    /// Host messages are left-aligned with a `[Host]` prefix.
    /// Human/agent messages are right-aligned by padding with leading spaces.
    #[instrument(skip(self))]
    fn to_line(&self, width: usize) -> Line<'static> {
        let style = Style::default().fg(self.participant.color());

        if self.participant.is_host() {
            let prefix = format!("[{}] ", self.participant.display_name());
            let available = width.saturating_sub(prefix.len());
            let text = clip_to_string(&self.text, available);
            Line::from(vec![
                Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                Span::styled(text, style),
            ])
        } else {
            let prefix = format!("{}: ", self.participant.display_name());
            let available = width.saturating_sub(prefix.len());
            let text = clip_to_string(&self.text, available);
            let label = format!("{prefix}{text}");
            let label_len = label.chars().count();
            let padding = " ".repeat(width.saturating_sub(label_len));
            Line::from(vec![
                Span::raw(padding),
                Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                Span::styled(text, style),
            ])
        }
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
    /// Creates a new [`ChatWidget`] with the default title `" 💬 Chat "`.
    #[instrument(skip(messages))]
    pub fn new(messages: &'a [ChatMessage]) -> Self {
        debug!(message_count = messages.len(), "Creating ChatWidget");
        Self {
            messages,
            title: " 💬 Chat ",
        }
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

        let width = inner.width as usize;

        let lines: Vec<Line<'static>> = if self.messages.is_empty() {
            debug!("No messages; rendering placeholder");
            vec![Line::from(Span::styled(
                "Waiting for first exchange…",
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            let max_rows = inner.height as usize;
            let start = self.messages.len().saturating_sub(max_rows);
            let visible = &self.messages[start..];

            debug!(
                total = self.messages.len(),
                visible = visible.len(),
                "Rendering chat messages"
            );

            visible.iter().map(|msg| msg.to_line(width)).collect()
        };

        let paragraph = Paragraph::new(lines).block(block);
        paragraph.render(area, buf);
    }
}

// ─────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────

/// Returns at most `max` chars from `s` as an owned [`String`].
#[instrument(skip(s))]
fn clip_to_string(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
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
