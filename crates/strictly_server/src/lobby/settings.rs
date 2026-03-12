//! Lobby settings — user-configurable preferences for the game session.

use tracing::instrument;

/// The game to play.
///
/// Defaults to [`GameType::TicTacToe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameType {
    /// Classic Tic-Tac-Toe.
    #[default]
    TicTacToe,
    /// Blackjack card game.
    Blackjack,
}

impl GameType {
    /// Display label shown in menus.
    #[instrument]
    pub fn label(self) -> &'static str {
        match self {
            Self::TicTacToe => "Tic-Tac-Toe",
            Self::Blackjack => "Blackjack",
        }
    }

    /// Short identifier used in database records.
    #[instrument]
    pub fn id(self) -> &'static str {
        match self {
            Self::TicTacToe => "tictactoe",
            Self::Blackjack => "blackjack",
        }
    }

    /// All available game types, in display order.
    #[instrument]
    pub fn all() -> &'static [GameType] {
        &[Self::TicTacToe, Self::Blackjack]
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LobbySettings {
    /// Who takes the first move in each game.
    pub first_player: FirstPlayer,
    /// Whether to show the typestate graph panel during games.
    pub show_typestate_graph: bool,
    /// Which game to play.
    pub selected_game: GameType,
}

impl Default for LobbySettings {
    #[instrument]
    fn default() -> Self {
        Self {
            first_player: FirstPlayer::default(),
            show_typestate_graph: false,
            selected_game: GameType::default(),
        }
    }
}

impl LobbySettings {
    /// Creates a new `LobbySettings` with defaults.
    #[instrument]
    pub fn new() -> Self {
        Self::default()
    }
}
