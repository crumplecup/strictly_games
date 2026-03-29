//! [`ElicitCommunicator`] backed by a language-model via [`LlmClient`].
//!
//! [`LlmElicitCommunicator`] converts every [`send_prompt`] call into a direct
//! LLM completion request.  It is the agent-side counterpart of
//! [`TuiCommunicator`]: the human types at the keyboard, the agent responds
//! through its LLM.
//!
//! # System prompt
//!
//! The agent is given a fixed system prompt that instructs it to act as a
//! blackjack player: respond concisely with the name or number of its chosen
//! option.  The elicitation runtime already appends the numbered option list to
//! the prompt, so the agent just needs to return a number or label.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::mcp_communicator::LlmElicitCommunicator;
//! use crate::AgentConfig;
//!
//! let comm = LlmElicitCommunicator::new(config)?;
//! ```

use elicitation::{
    ElicitCommunicator, ElicitError, ElicitErrorKind, ElicitResult, ElicitationContext,
    StyleContext, StyleMarker,
};
use tracing::{debug, info, instrument, warn};

use crate::AgentConfig;
use crate::llm_client::LlmClient;

/// System prompt given to all agent players.
const BLACKJACK_SYSTEM_PROMPT: &str = "\
You are an AI agent playing blackjack. You will be asked to make game decisions \
(e.g. place a bet, hit or stand). Reply with ONLY the option number or option label — \
no explanation, no punctuation, nothing else. \
For example, if asked to choose between Hit and Stand, reply with '1' or 'Hit'.";

/// An [`ElicitCommunicator`] that sends prompts directly to an LLM.
///
/// Each [`send_prompt`] call translates to one LLM completion request.
/// The agent's response is returned as-is to the elicitation runtime,
/// which validates it against the expected options.
///
/// Constructed from an [`AgentConfig`] which provides the LLM provider,
/// model, and API key via environment variables.
#[derive(Clone)]
pub struct LlmElicitCommunicator {
    client: LlmClient,
    agent_name: String,
    style_ctx: StyleContext,
    elicit_ctx: ElicitationContext,
}

impl LlmElicitCommunicator {
    /// Creates a new communicator from the given agent configuration.
    ///
    /// # Errors
    ///
    /// Returns an error string if the LLM config cannot be built (e.g.
    /// missing API key environment variable).
    #[instrument(skip(config), fields(agent_name = %config.name()))]
    pub fn new(config: &AgentConfig) -> Result<Self, String> {
        info!("Creating LlmElicitCommunicator");
        let llm_config = config.create_llm_config().map_err(|e| e.to_string())?;
        let client = LlmClient::new(llm_config);
        Ok(Self {
            client,
            agent_name: config.name().clone(),
            style_ctx: StyleContext::default(),
            elicit_ctx: ElicitationContext::default(),
        })
    }
}

impl ElicitCommunicator for LlmElicitCommunicator {
    /// Send the prompt to the LLM and return its response.
    ///
    /// The elicitation runtime has already formatted `prompt` with the full
    /// numbered options list, so the LLM just needs to return a number or label.
    #[instrument(skip(self), fields(agent = %self.agent_name, prompt_len = prompt.len()))]
    fn send_prompt(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = ElicitResult<String>> + Send {
        let client = self.client.clone();
        let prompt_owned = prompt.to_string();
        let agent_name = self.agent_name.clone();

        async move {
            debug!(agent = %agent_name, "Sending prompt to LLM");

            let response = client
                .generate(BLACKJACK_SYSTEM_PROMPT, &prompt_owned)
                .await
                .map_err(|e| {
                    warn!(error = %e, agent = %agent_name, "LLM generation failed");
                    ElicitError::new(ElicitErrorKind::ParseError(format!(
                        "LLM error for agent {agent_name}: {e}"
                    )))
                })?;

            let trimmed = response.trim().to_string();
            info!(
                agent = %agent_name,
                response = %trimmed,
                "LLM response received"
            );
            Ok(trimmed)
        }
    }

    #[instrument(skip(self, _params), level = "debug")]
    fn call_tool(
        &self,
        _params: rmcp::model::CallToolRequestParams,
    ) -> impl std::future::Future<
        Output = Result<rmcp::model::CallToolResult, rmcp::service::ServiceError>,
    > + Send {
        let agent_name = self.agent_name.clone();
        async move {
            warn!(agent = %agent_name, "call_tool invoked on LlmElicitCommunicator — not supported");
            Err(rmcp::service::ServiceError::Cancelled {
                reason: Some("LLM communicator does not support MCP tool calls".to_string()),
            })
        }
    }

    #[instrument(skip(self))]
    fn style_context(&self) -> &StyleContext {
        &self.style_ctx
    }

    #[instrument(skip(self))]
    fn elicitation_context(&self) -> &ElicitationContext {
        &self.elicit_ctx
    }

    #[instrument(skip(self, style), level = "debug")]
    fn with_style<T: 'static, S: StyleMarker + elicitation::style::ElicitationStyle + 'static>(
        &self,
        style: S,
    ) -> Self {
        let mut new = self.clone();
        new.style_ctx.set_style::<T, S>(style).ok();
        new
    }
}
