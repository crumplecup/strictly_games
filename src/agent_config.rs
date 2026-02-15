//! Agent configuration for MCP client.

use crate::llm_client::{LlmConfig, LlmProvider};
use derive_getters::Getters;
use derive_more::{Display, Error};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, instrument};

/// Configuration for an MCP agent client.
#[derive(Debug, Clone, Getters, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent name (used in game sessions).
    name: String,

    /// Command to start the MCP server (as array).
    server_command: Vec<String>,

    /// Optional working directory for server process.
    server_cwd: Option<String>,

    /// LLM provider (openai or anthropic).
    #[serde(default = "default_provider")]
    llm_provider: LlmProvider,

    /// LLM model name (e.g., "gpt-4", "claude-3-5-sonnet").
    #[serde(default = "default_model")]
    llm_model: String,

    /// Maximum tokens for LLM responses.
    #[serde(default = "default_max_tokens")]
    llm_max_tokens: u32,
}

#[instrument]
fn default_provider() -> LlmProvider {
    LlmProvider::OpenAI
}

#[instrument]
fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

#[instrument]
fn default_max_tokens() -> u32 {
    150
}

impl AgentConfig {
    /// Creates a new agent configuration.
    #[instrument(skip(name, server_command, server_cwd), fields(agent_name = %name))]
    pub fn new(
        name: String,
        server_command: Vec<String>,
        server_cwd: Option<String>,
    ) -> Self {
        Self {
            name,
            server_command,
            server_cwd,
            llm_provider: default_provider(),
            llm_model: default_model(),
            llm_max_tokens: default_max_tokens(),
        }
    }

    /// Loads configuration from TOML file.
    #[instrument(skip(path), fields(path = %path.as_ref().display()))]
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        debug!("Loading config from file");
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            ConfigError::new(format!("Failed to read config file: {}", e))
        })?;

        let config: Self = toml::from_str(&content).map_err(|e| {
            ConfigError::new(format!("Failed to parse config: {}", e))
        })?;

        info!(agent_name = %config.name, "Config loaded successfully");
        Ok(config)
    }

    /// Creates LLM configuration from this agent config.
    /// Requires OPENAI_API_KEY or ANTHROPIC_API_KEY environment variable.
    #[instrument(skip(self), fields(provider = ?self.llm_provider, model = %self.llm_model))]
    pub fn create_llm_config(&self) -> Result<LlmConfig, ConfigError> {
        debug!("Creating LLM config");

        let api_key = match self.llm_provider {
            LlmProvider::OpenAI => std::env::var("OPENAI_API_KEY").map_err(|_| {
                ConfigError::new("OPENAI_API_KEY environment variable not set".to_string())
            })?,
            LlmProvider::Anthropic => std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                ConfigError::new("ANTHROPIC_API_KEY environment variable not set".to_string())
            })?,
        };

        Ok(LlmConfig::new(
            self.llm_provider,
            api_key,
            self.llm_model.clone(),
            self.llm_max_tokens,
        ))
    }
}

/// Configuration error.
#[derive(Debug, Clone, Display, Error)]
#[display("Config error: {} at {}:{}", message, file, line)]
pub struct ConfigError {
    /// Error message.
    pub message: String,
    /// Line number where error occurred.
    pub line: u32,
    /// Source file where error occurred.
    pub file: &'static str,
}

impl ConfigError {
    /// Creates a new configuration error.
    #[track_caller]
    #[instrument(skip(message))]
    pub fn new(message: String) -> Self {
        let loc = std::panic::Location::caller();
        Self {
            message,
            line: loc.line(),
            file: loc.file(),
        }
    }
}
