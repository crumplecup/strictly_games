//! Communicator wrapper that accumulates game knowledge and embeds it
//! into every subsequent elicitation prompt.
//!
//! # Problem
//!
//! Each MCP `create_message()` call is stateless — the agent does not
//! carry context from one sampling request to the next.  When an agent
//! explores (e.g. "view board", "view threats"), the description is sent
//! via `send_prompt()`, but the NEXT elicitation prompt starts fresh.
//! The agent effectively forgets what it just learned.
//!
//! # Solution
//!
//! [`ContextualCommunicator`] wraps any inner communicator and maintains
//! a growing `knowledge` cache.  Each prompt is prefixed with all
//! accumulated knowledge so the agent sees everything it has learned in
//! every subsequent interaction.
//!
//! The knowledge cache is append-only within a turn.  A fresh cache is
//! created for each decision point, so no explicit clearing is needed.
//!
//! Since game contexts are small (tic-tac-toe board, blackjack hand),
//! carrying the full cache costs very few tokens.

use elicitation::{
    ElicitCommunicator, ElicitResult, ElicitationContext, StyleContext, StyleMarker,
};
use std::sync::{Arc, Mutex};

/// Accumulated game knowledge that persists across elicitation rounds.
///
/// Shared via `Arc<Mutex<>>` so the outer game loop can append explore
/// results and the communicator can read them during `send_prompt()`.
#[derive(Debug, Clone, Default)]
pub struct KnowledgeCache {
    /// Ordered list of knowledge entries, newest last.
    entries: Vec<String>,
}

impl KnowledgeCache {
    /// Appends a new piece of knowledge.
    pub fn push(&mut self, entry: impl Into<String>) {
        self.entries.push(entry.into());
    }

    /// Formats the full knowledge cache as a prompt preamble.
    fn format_preamble(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut preamble = String::from("[Previously gathered game knowledge]\n");
        for (i, entry) in self.entries.iter().enumerate() {
            preamble.push_str(&format!("{}. {}\n", i + 1, entry));
        }
        preamble.push('\n');
        preamble
    }
}

/// Thread-safe handle to a shared knowledge cache.
pub type SharedKnowledge = Arc<Mutex<KnowledgeCache>>;

/// Creates a new shared knowledge cache.
pub fn knowledge_cache() -> SharedKnowledge {
    Arc::new(Mutex::new(KnowledgeCache::default()))
}

/// Communicator wrapper that prepends accumulated game knowledge to
/// every prompt sent to the agent.
///
/// See the [module documentation][self] for design rationale.
#[derive(Clone)]
pub struct ContextualCommunicator<C> {
    inner: C,
    knowledge: SharedKnowledge,
}

impl<C> ContextualCommunicator<C> {
    /// Wraps `inner` with a shared knowledge cache.
    pub fn new(inner: C, knowledge: SharedKnowledge) -> Self {
        Self { inner, knowledge }
    }
}

impl<C: ElicitCommunicator + Clone> ElicitCommunicator for ContextualCommunicator<C> {
    /// Prepends accumulated knowledge to the prompt, then delegates.
    fn send_prompt(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = ElicitResult<String>> + Send {
        let preamble = {
            let cache = self.knowledge.lock().unwrap();
            cache.format_preamble()
        };
        let enriched = if preamble.is_empty() {
            prompt.to_string()
        } else {
            format!("{preamble}{prompt}")
        };
        // Clone inner so the async block owns both the communicator
        // and the enriched string — avoids lifetime issues with &str.
        let inner = self.inner.clone();
        tracing::debug!(prompt_len = enriched.len(), "Sending enriched prompt");

        async move { inner.send_prompt(&enriched).await }
    }

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

    fn with_style<T: 'static, S: StyleMarker + elicitation::style::ElicitationStyle + 'static>(
        &self,
        style: S,
    ) -> Self {
        Self {
            inner: self.inner.with_style::<T, S>(style),
            knowledge: self.knowledge.clone(),
        }
    }
}
