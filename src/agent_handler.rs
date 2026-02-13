//! MCP ClientHandler implementation for game agent.

use crate::agent_config::AgentConfig;
use crate::llm_client::LlmClient;
use rmcp::handler::client::ClientHandler;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleClient};
use rmcp::ErrorData;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

/// Game agent MCP client.
#[derive(Clone)]
pub struct GameAgent {
    config: AgentConfig,
    llm_client: Arc<Mutex<Option<LlmClient>>>,
}

impl GameAgent {
    /// Create a new game agent.
    #[instrument(skip(config))]
    pub fn new(config: AgentConfig) -> Self {
        tracing::debug!(agent_name = %config.name(), "Creating GameAgent");
        Self {
            config,
            llm_client: Arc::new(Mutex::new(None)),
        }
    }

    /// Initialize LLM client.
    #[instrument(skip(self))]
    pub async fn initialize_llm(&self) -> Result<(), String> {
        tracing::info!("Initializing LLM client");

        let llm_config = self
            .config
            .create_llm_config()
            .map_err(|e| e.to_string())?;

        let client = LlmClient::new(llm_config);

        let mut guard = self.llm_client.lock().await;
        *guard = Some(client);

        tracing::info!("LLM client initialized");
        Ok(())
    }
}

impl ClientHandler for GameAgent {
    #[instrument(skip(self, _context))]
    fn ping(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<(), ErrorData>> + Send + '_ {
        tracing::debug!("Handling ping");
        async { Ok(()) }
    }

    #[instrument(skip(self, params, _context), fields(num_messages = params.messages.len()))]
    fn create_message(
        &self,
        params: CreateMessageRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<CreateMessageResult, ErrorData>> + Send + '_ {
        let llm_client = self.llm_client.clone();
        let config = self.config.clone();

        async move {
            tracing::info!("Handling create_message (sampling) with LLM");

            // Get LLM client
            let guard = llm_client.lock().await;
            let client = guard.as_ref().ok_or_else(|| {
                tracing::error!("LLM client not initialized");
                ErrorData::internal_error("LLM client not initialized", None)
            })?;

            // Extract user message from params
            let user_message = params
                .messages
                .iter()
                .filter_map(|msg| {
                    if let SamplingContent::Single(SamplingMessageContent::Text(text)) =
                        &msg.content
                    {
                        Some(text.text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            if user_message.is_empty() {
                tracing::warn!("Empty message content");
                return Err(ErrorData::invalid_params("Empty message content", None));
            }

            tracing::debug!(message_length = user_message.len(), "Processing message");

            // Call LLM
            let system_prompt = format!(
                "You are {}, an AI agent playing games via MCP. \
                 When asked to make a move, respond with ONLY the position number (0-8) and nothing else.",
                config.name()
            );

            let response = client
                .generate(&system_prompt, &user_message)
                .await
                .map_err(|e| {
                    tracing::error!(error = ?e, "LLM generation failed");
                    ErrorData::internal_error(e.to_string(), None)
                })?;

            tracing::info!(response_length = response.len(), "LLM response received");

            // Return as CreateMessageResult
            Ok(CreateMessageResult {
                model: config.llm_model().to_string(),
                stop_reason: Some("endTurn".to_string()),
                message: SamplingMessage {
                    role: Role::Assistant,
                    content: SamplingContent::Single(SamplingMessageContent::Text(
                        RawTextContent {
                            text: response,
                            meta: None,
                        },
                    )),
                    meta: None,
                },
            })
        }
    }

    #[instrument(skip(self, _context))]
    fn list_roots(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<ListRootsResult, ErrorData>> + Send + '_ {
        tracing::debug!("Handling list_roots");
        async { Ok(ListRootsResult::default()) }
    }

    #[instrument(skip(self))]
    fn get_info(&self) -> <RoleClient as rmcp::service::ServiceRole>::Info {
        tracing::debug!("Providing client info");
        InitializeRequestParams {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: ClientCapabilities {
                sampling: None,  // TODO: Enable when implementing LLM client
                roots: None,
                experimental: None,
                elicitation: None,
                extensions: None,
                tasks: None,
            },
            client_info: Implementation {
                name: self.config.name().clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: Some("MCP game agent".to_string()),
                icons: None,
                website_url: None,
            },
            meta: None,
        }
    }
}
