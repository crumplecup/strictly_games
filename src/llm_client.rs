//! LLM API client abstraction for OpenAI and Anthropic.

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client as OpenAIClient,
};
use derive_more::{Display, Error};
use reqwest;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};

/// LLM provider selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// OpenAI (GPT models).
    OpenAI,
    /// Anthropic (Claude models).
    Anthropic,
}

/// Configuration for LLM client.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    provider: LlmProvider,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl LlmConfig {
    /// Creates a new LLM configuration.
    #[instrument(skip(api_key), fields(provider = ?provider, model = %model))]
    pub fn new(provider: LlmProvider, api_key: String, model: String, max_tokens: u32) -> Self {
        debug!("Creating LLM config");
        Self {
            provider,
            api_key,
            model,
            max_tokens,
        }
    }

    /// Gets the provider.
    #[instrument(skip(self))]
    pub fn provider(&self) -> LlmProvider {
        self.provider
    }

    /// Gets the API key.
    #[instrument(skip(self))]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Gets the model name.
    #[instrument(skip(self))]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Gets the max tokens.
    #[instrument(skip(self))]
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }
}

/// LLM client that abstracts over multiple providers.
#[derive(Debug, Clone)]
pub struct LlmClient {
    config: LlmConfig,
}

impl LlmClient {
    /// Creates a new LLM client.
    #[instrument(skip(config), fields(provider = ?config.provider()))]
    pub fn new(config: LlmConfig) -> Self {
        info!("Creating LLM client");
        Self { config }
    }

    /// Generates a completion from a system prompt and user message.
    #[instrument(skip(self, system_prompt, user_message), fields(provider = ?self.config.provider, model = %self.config.model))]
    pub async fn generate(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, LlmError> {
        debug!("Generating completion");
        match self.config.provider {
            LlmProvider::OpenAI => self.generate_openai(system_prompt, user_message).await,
            LlmProvider::Anthropic => self.generate_anthropic(system_prompt, user_message).await,
        }
    }

    /// Generates a completion using Anthropic Claude.
    #[instrument(skip(self, system_prompt, user_message))]
    async fn generate_anthropic(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, LlmError> {
        debug!("Creating Anthropic client");

        let client = reqwest::Client::new();

        debug!("Building Anthropic API request");
        let request_body = serde_json::json!({
            "model": self.config.model,
            "max_tokens": self.config.max_tokens,
            "system": system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": user_message
                }
            ]
        });

        debug!("Sending request to Anthropic");
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.config.api_key.clone())
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(error = ?e, "Anthropic API request failed");
                LlmError::new(format!("Anthropic API request failed: {}", e))
            })?;

        let status = response.status();
        let response_text = response.text().await.map_err(|e| {
            error!(error = ?e, "Failed to read Anthropic response");
            LlmError::new(format!("Failed to read response: {}", e))
        })?;

        if !status.is_success() {
            error!(status = %status, response = %response_text, "Anthropic API error");
            return Err(LlmError::new(format!(
                "Anthropic API error {}: {}",
                status, response_text
            )));
        }

        debug!(response_length = response_text.len(), "Parsing Anthropic response");
        let response_json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
            error!(error = ?e, response = %response_text, "Failed to parse Anthropic response");
            LlmError::new(format!("Failed to parse response: {}", e))
        })?;

        let content = response_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                error!(response = %response_json, "No text content in Anthropic response");
                LlmError::new("No text content in Anthropic response".to_string())
            })?
            .to_string();

        info!(content_length = content.len(), "Generated completion");
        Ok(content)
    }

    /// Generates a completion using OpenAI.
    #[instrument(skip(self, system_prompt, user_message))]
    async fn generate_openai(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, LlmError> {
        debug!("Creating OpenAI client");

        let client = OpenAIClient::with_config(
            OpenAIConfig::new().with_api_key(self.config.api_key.clone()),
        );

        debug!("Building chat completion request");
        let messages = vec![
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(system_prompt)
                    .build()
                    .map_err(|e| {
                        error!(error = ?e, "Failed to build system message");
                        LlmError::new(format!("Failed to build system message: {}", e))
                    })?,
            ),
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(user_message)
                    .build()
                    .map_err(|e| {
                        error!(error = ?e, "Failed to build user message");
                        LlmError::new(format!("Failed to build user message: {}", e))
                    })?,
            ),
        ];

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.config.model)
            .messages(messages)
            .max_tokens(self.config.max_tokens)
            .build()
            .map_err(|e| {
                error!(error = ?e, "Failed to build request");
                LlmError::new(format!("Failed to build request: {}", e))
            })?;

        debug!("Sending request to OpenAI");
        let response = client.chat().create(request).await.map_err(|e| {
            error!(error = ?e, "OpenAI API error");
            LlmError::new(format!("OpenAI API error: {}", e))
        })?;

        let content = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| {
                error!("No content in OpenAI response");
                LlmError::new("No content in OpenAI response".to_string())
            })?;

        info!(content_length = content.len(), "Generated completion");
        Ok(content)
    }
}

/// LLM client error.
#[derive(Debug, Clone, Display, Error)]
#[display("LLM error: {} at {}:{}", message, file, line)]
pub struct LlmError {
    pub message: String,
    pub line: u32,
    pub file: &'static str,
}

impl LlmError {
    /// Creates a new LLM error.
    #[track_caller]
    #[instrument(skip(message))]
    pub fn new(message: String) -> Self {
        let loc = std::panic::Location::caller();
        error!(error_message = %message, "LLM error created");
        Self {
            message,
            line: loc.line(),
            file: loc.file(),
        }
    }
}
