//! Agent preset library — scans a config directory for agent `.toml` files.

use std::path::{Path, PathBuf};

use tracing::{debug, info, instrument, warn};

use crate::{AgentConfig, ConfigError};

/// A scanned collection of agent configurations.
///
/// Use [`AgentLibrary::scan`] to load from a directory, or
/// [`AgentLibrary::scan_default`] to use the default config location.
#[derive(Debug, Clone)]
pub struct AgentLibrary {
    agents: Vec<AgentConfig>,
}

impl AgentLibrary {
    /// Creates an empty agent library.
    #[instrument]
    pub fn empty() -> Self {
        Self { agents: Vec::new() }
    }

    /// Scans `dir_path` for `*.toml` files and loads each as an [`AgentConfig`].
    ///
    /// Invalid or non-TOML files are skipped with a warning. Returns an error
    /// only if the directory cannot be read or contains no valid configs.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the path does not exist, is not a directory,
    /// cannot be read, or yields no valid agent configs.
    #[instrument(skip(dir_path), fields(path = %dir_path.as_ref().display()))]
    pub fn scan(dir_path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = dir_path.as_ref();
        info!(path = %path.display(), "Scanning directory for agent configs");

        if !path.exists() {
            warn!(path = %path.display(), "Path does not exist");
            return Err(ConfigError::new(format!(
                "Agent config directory not found: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            warn!(path = %path.display(), "Path is not a directory");
            return Err(ConfigError::new(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        debug!("Reading directory entries");
        let entries = std::fs::read_dir(path).map_err(|e| {
            warn!(error = %e, "Failed to read directory");
            ConfigError::new(format!(
                "Failed to read directory {}: {}",
                path.display(),
                e
            ))
        })?;

        let mut agents = Vec::new();

        for entry_result in entries {
            let entry = entry_result
                .map_err(|e| ConfigError::new(format!("Failed to read directory entry: {}", e)))?;

            let entry_path = entry.path();

            if !entry_path.is_file() {
                debug!(path = %entry_path.display(), "Skipping non-file entry");
                continue;
            }

            if entry_path.extension().and_then(|s| s.to_str()) != Some("toml") {
                debug!(path = %entry_path.display(), "Skipping non-TOML file");
                continue;
            }

            match AgentConfig::from_file(&entry_path) {
                Ok(config) => {
                    info!(
                        name = %config.name(),
                        path = %entry_path.display(),
                        "Loaded agent config"
                    );
                    agents.push(config);
                }
                Err(e) => {
                    warn!(
                        path = %entry_path.display(),
                        error = %e,
                        "Skipping invalid agent config"
                    );
                }
            }
        }

        if agents.is_empty() {
            return Err(ConfigError::new(format!(
                "No valid agent configs found in: {}",
                path.display()
            )));
        }

        // Sort by name for stable ordering across platforms.
        agents.sort_by(|a, b| a.name().cmp(b.name()));

        info!(count = agents.len(), "Agent library loaded");
        Ok(Self { agents })
    }

    /// Scans the default agent config directory.
    ///
    /// Resolution order:
    /// 1. `$STRICTLY_GAMES_AGENTS` environment variable
    /// 2. `$XDG_CONFIG_HOME/strictly_games/agents`
    /// 3. `./examples` (development fallback)
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if the resolved directory contains no valid configs.
    #[instrument]
    pub fn scan_default() -> Result<Self, ConfigError> {
        let dir = Self::default_config_dir();
        info!(path = %dir.display(), "Scanning default config directory");
        Self::scan(dir)
    }

    /// Returns the default config directory path using the resolution order
    /// documented on [`AgentLibrary::scan_default`].
    #[instrument]
    pub fn default_config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("STRICTLY_GAMES_AGENTS") {
            debug!(path = %dir, "Using STRICTLY_GAMES_AGENTS env var");
            return PathBuf::from(dir);
        }

        // Try workspace structure first (crates/strictly_games/examples)
        let workspace_path = PathBuf::from("crates/strictly_games/examples");
        if workspace_path.exists() && workspace_path.is_dir() {
            debug!(path = %workspace_path.display(), "Using workspace examples directory");
            return workspace_path;
        }

        // Try local ./examples (when running from crate directory)
        let local_path = PathBuf::from("examples");
        if local_path.exists() && local_path.is_dir() {
            debug!(path = %local_path.display(), "Using local examples directory");
            return local_path;
        }

        // Fall back to XDG config
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let dir = PathBuf::from(xdg).join("strictly_games").join("agents");
            debug!(path = %dir.display(), "Using XDG_CONFIG_HOME path");
            return dir;
        }

        debug!("No config directory found, returning ./examples");
        PathBuf::from("examples")
    }

    /// Returns all loaded agent configs, sorted by name.
    #[instrument(skip(self))]
    pub fn agents(&self) -> &[AgentConfig] {
        &self.agents
    }

    /// Looks up an agent config by exact name.
    #[instrument(skip(self))]
    pub fn get_by_name(&self, name: &str) -> Option<&AgentConfig> {
        debug!(name = %name, "Looking up agent by name");
        self.agents.iter().find(|a| a.name() == name)
    }

    /// Returns the number of loaded agents.
    #[instrument(skip(self))]
    pub fn count(&self) -> usize {
        self.agents.len()
    }
    #[instrument(skip(self))]
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Returns `true` if no agents are loaded.
    #[instrument(skip(self))]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}
