//! Transparent middleware that captures the exact prompt text sent to any
//! [`ElicitCommunicator`] and broadcasts it over a [`watch`] channel.
//! Optionally also appends each exchange to a [`chat_widget`] history.
//!
//! # Design
//!
//! [`ObservableCommunicator`] wraps any inner communicator and forwards every
//! call unchanged. The only side-effect is in [`send_prompt`]: before
//! delegating to the inner communicator, it writes the fully-assembled prompt
//! string — with numbered options already appended by the elicitation
//! runtime — to a `watch::Sender<Option<String>>`.
//!
//! If a chat sender is also provided, each exchange is appended:
//! - `ChatMessage(Host, prompt)` — before the inner communicator is called
//! - `ChatMessage(participant, response)` — after the response arrives
//!
//! ## Why `watch` for the in-flight prompt?
//!
//! `watch` is the idiomatic tokio primitive for "latest value" semantics:
//! non-blocking to read, no backpressure, automatically drops stale values.
//!
//! ## Why `mpsc` for chat history?
//!
//! The chat log is append-only; every message must survive. `mpsc` preserves
//! ordering and never drops old entries.
//!
//! # Usage
//!
//! ```rust,ignore
//! use tokio::sync::{mpsc, watch};
//! use crate::tui::chat_widget::{ChatMessage, Participant, chat_channel};
//! use crate::tui::observable_communicator::ObservableCommunicator;
//! use crate::tui::tui_communicator::TuiCommunicator;
//!
//! let (prompt_tx, prompt_rx) = watch::channel(None);
//! let (chat_tx, chat_rx) = chat_channel();
//! let comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx)
//!     .with_chat(chat_tx, Participant::Human);
//! ```

use elicitation::{
    ElicitCommunicator, ElicitResult, ElicitationContext, StyleMarker, StyleContext,
};
use tokio::sync::{mpsc, watch};
use tracing::instrument;

use crate::tui::chat_widget::{ChatMessage, Participant};

/// Transparent middleware that captures each prompt sent through the inner
/// communicator and publishes it to a [`watch`] channel.
///
/// Optionally forwards each exchange to a shared chat history channel.
///
/// See the [module documentation][self] for design rationale and usage.
#[derive(Clone)]
pub struct ObservableCommunicator<C> {
    inner: C,
    sender: watch::Sender<Option<String>>,
    /// Optional chat history sink and participant identity.
    chat: Option<(mpsc::UnboundedSender<ChatMessage>, Participant)>,
}

impl<C> ObservableCommunicator<C> {
    /// Wraps `inner` and publishes each in-flight prompt to `sender`.
    pub fn new(inner: C, sender: watch::Sender<Option<String>>) -> Self {
        Self {
            inner,
            sender,
            chat: None,
        }
    }

    /// Attaches a chat-history sink so each exchange is appended as messages.
    ///
    /// `participant` identifies who is replying (e.g. `Participant::Human` or
    /// `Participant::Agent("GPT-4o".into())`).
    pub fn with_chat(
        mut self,
        chat_tx: mpsc::UnboundedSender<ChatMessage>,
        participant: Participant,
    ) -> Self {
        self.chat = Some((chat_tx, participant));
        self
    }
}

impl<C: ElicitCommunicator> ElicitCommunicator for ObservableCommunicator<C> {
    /// Publish the prompt to the watch channel (and chat log), delegate to
    /// the inner communicator, then clear the watch channel on return.
    #[instrument(skip(self), level = "debug", fields(prompt_len = prompt.len()))]
    fn send_prompt(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = ElicitResult<String>> + Send {
        let prompt_owned = prompt.to_string();
        let watch_tx = self.sender.clone();
        let chat = self.chat.clone();
        let inner_future = self.inner.send_prompt(prompt);

        async move {
            // Publish in-flight prompt to the typestate widget.
            watch_tx.send(Some(prompt_owned.clone())).ok();

            // Append host prompt to chat history.
            if let Some((ref tx, _)) = chat {
                tx.send(ChatMessage::new(Participant::Host, prompt_owned))
                    .ok();
            }

            let result = inner_future.await;

            // Clear the in-flight prompt: exchange complete.
            watch_tx.send(None).ok();

            // Append the reply to chat history.
            if let Some((ref tx, ref participant)) = chat
                && let Ok(ref response) = result
            {
                tx.send(ChatMessage::new(participant.clone(), response.clone()))
                    .ok();
            }

            result
        }
    }

    #[instrument(skip(self, params), level = "debug")]
    fn call_tool(
        &self,
        params: rmcp::model::CallToolRequestParams,
    ) -> impl std::future::Future<
        Output = Result<rmcp::model::CallToolResult, rmcp::service::ServiceError>,
    > + Send {
        self.inner.call_tool(params)
    }

    fn style_context(&self) -> &StyleContext {
        self.inner.style_context()
    }

    fn elicitation_context(&self) -> &ElicitationContext {
        self.inner.elicitation_context()
    }

    fn with_style<T: 'static, S: StyleMarker + elicitation::style::ElicitationStyle + 'static>(&self, style: S) -> Self {
        Self {
            inner: self.inner.with_style::<T, S>(style),
            sender: self.sender.clone(),
            chat: self.chat.clone(),
        }
    }
}
