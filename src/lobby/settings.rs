//! Lobby settings — user-configurable preferences for the game session.

use tracing::instrument;

/// Which player takes the first move (X) in a new game.
///
/// Defaults to [`FirstPlayer::Human`] so the player moves first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FirstPlayer {
    /// The human player registers first and plays as X.
    #[default]
    Human,
    /// The agent registers first and plays as X.
    Agent,
}

impl FirstPlayer {
    /// Returns the display label for this option.
    #[instrument]
    pub fn label(self) -> &'static str {
        match self {
            Self::Human => "Player",
            Self::Agent => "Agent",
        }
    }

    /// Toggles between `Human` and `Agent`.
    #[instrument]
    pub fn toggle(self) -> Self {
        match self {
            Self::Human => Self::Agent,
            Self::Agent => Self::Human,
        }
    }
}

/// User-configurable settings for the lobby.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LobbySettings {
    /// Who takes the first move in each game.
    pub first_player: FirstPlayer,
}

impl LobbySettings {
    /// Creates a new `LobbySettings` with defaults.
    #[instrument]
    pub fn new() -> Self {
        Self::default()
    }
}
