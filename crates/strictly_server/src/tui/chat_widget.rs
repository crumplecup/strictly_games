//! Chat widget for rendering the elicitation exchange between the host and players.
//!
//! Displays a scrolling chat log in a bordered ratatui panel, newest messages at the bottom.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
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
}

// ─────────────────────────────────────────────────────────────
//  ChatWidget
// ─────────────────────────────────────────────────────────────

/// Renders the elicitation chat log as an ratatui widget.
///
/// Displays messages newest-at-bottom within a bordered panel, colour-coded by
/// participant. Host messages are left-aligned; human and agent replies are
/// right-aligned.
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
//  Widget impl
// ─────────────────────────────────────────────────────────────

impl Widget for ChatWidget<'_> {
    #[instrument(skip(self, buf))]
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).title(self.title);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if self.messages.is_empty() {
            debug!("No messages; rendering placeholder");
            let style = Style::default().fg(Color::DarkGray);
            buf.set_string(
                inner.x,
                inner.y,
                clip("Waiting for first exchange…", inner.width as usize),
                style,
            );
            return;
        }

        let max_rows = inner.height as usize;
        let start = self.messages.len().saturating_sub(max_rows);
        let visible = &self.messages[start..];

        debug!(
            total = self.messages.len(),
            visible = visible.len(),
            "Rendering chat messages"
        );

        for (i, msg) in visible.iter().enumerate() {
            let row = inner.y + i as u16;
            let style = Style::default().fg(msg.participant.color());
            let width = inner.width as usize;

            if msg.participant.is_host() {
                // "[Host] {text}" — left-aligned
                let prefix = format!("[{}] ", msg.participant.display_name());
                let available = width.saturating_sub(prefix.len());
                let label = format!("{}{}", prefix, clip(&msg.text, available));
                buf.set_string(inner.x, row, &label, style);
            } else {
                // "{name}: {text}" — right-aligned (human = "You", agent = model name)
                let prefix = format!("{}: ", msg.participant.display_name());
                render_right(buf, inner.x, row, width, &prefix, &msg.text, style);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────

/// Renders a right-aligned `"{prefix}{text}"` label into `buf` at the given row.
#[instrument(skip(buf, prefix, text, style))]
fn render_right(
    buf: &mut Buffer,
    area_x: u16,
    row: u16,
    width: usize,
    prefix: &str,
    text: &str,
    style: Style,
) {
    let available = width.saturating_sub(prefix.len());
    let clipped_text = clip(text, available);
    let label = format!("{}{}", prefix, clipped_text);
    let label_len = label.chars().count();
    let x_offset = width.saturating_sub(label_len) as u16;
    buf.set_string(area_x + x_offset, row, &label, style);
}

/// Returns at most `max` chars from `s` (char-boundary safe).
#[instrument(skip(s))]
fn clip(s: &str, max: usize) -> &str {
    if max == 0 {
        return "";
    }
    let mut char_indices = s.char_indices();
    let mut last_byte = s.len();
    for (count, (byte_pos, _)) in char_indices.by_ref().enumerate() {
        if count == max {
            last_byte = byte_pos;
            break;
        }
    }
    &s[..last_byte]
}

// ─────────────────────────────────────────────────────────────
//  Channel helper
// ─────────────────────────────────────────────────────────────

/// Creates a bounded chat channel (capacity 256).
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
