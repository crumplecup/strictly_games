//! Chat channel and participant types for multi-player sessions.
//!
//! Provides [`Participant`], [`ChatMessage`], and [`chat_channel`] for routing
//! in-game chat through the elicitation framework.  Messages are converted to
//! [`crate::session::DialogueEntry`] values for inclusion in the AccessKit IR.

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
