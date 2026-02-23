//! Strictly Server - Game orchestration and networking
//!
//! This library provides:
//! - **MCP server** for LLM agent integration
//! - **REST API** for game operations
//! - **Database** persistence for users and stats
//! - **Session management** for multiplayer games
//! - **Lobby system** with TUI
//!
//! Uses `strictly_tictactoe` for pure game logic.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

// Private module declarations
mod agent_config;
mod agent_handler;
mod agent_library;
mod db;
mod games;
mod llm_client;
mod lobby;
mod profile_service;
mod server;
mod session;
mod tui;

// Public API exports - Agent configuration
pub use agent_config::{AgentConfig, ConfigError};

// Public API exports - Agent library
pub use agent_library::AgentLibrary;

// Public API exports - Agent handler
pub use agent_handler::GameAgent;

// Public API exports - Database
pub use db::{
    AggregatedStats, DbError, GameOutcome, GameRepository, GameStat, NewGameStat, NewUser, User,
};

// Public API exports - Lobby
pub use lobby::{FirstPlayer, LobbyController, LobbySettings, Screen, ScreenTransition};

// Public API exports - Profile service
pub use profile_service::ProfileService;

// Public API exports - LLM client
pub use llm_client::LlmClient;

// Public API exports - Server
pub use server::GameServer;

// Public API exports - Session
pub use session::GameSession;

// Public API exports - TUI
pub use tui::run_game_session;

// Public API exports - Game types
pub use games::tictactoe::{
    AnyGame, Mark, Player, Position, Board, Square,
    GameSetup, GameInProgress, GameFinished,
};

// Re-export for convenience
pub use strictly_tictactoe;

// Type aliases for compatibility
/// Player type for TUI
pub type TicTacToePlayer = Player;
