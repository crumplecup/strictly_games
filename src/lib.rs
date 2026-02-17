//! Strictly Games library - type-safe game implementations
//!
//! This library provides MCP-based game servers with AI agent support.
//!
//! # Architecture
//!
//! - **Server**: MCP server for game sessions (stdio or HTTP)
//! - **Agent**: AI players using LLM APIs (OpenAI, Anthropic)
//! - **Games**: Type-safe game implementations (currently tic-tac-toe)
//! - **Session**: Multi-player session management
//! - **Typestates**: Compile-time state machine enforcement
//!
//! # Example
//!
//! ```no_run
//! use strictly_games::{GameServer, AgentConfig, LlmProvider};
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create game server
//! let server = GameServer::new();
//!
//! // Configure AI agent
//! let config = AgentConfig::new(
//!     "agent1".to_string(),
//!     vec!["strictly_games".to_string(), "--server".to_string()],
//!     None,
//! );
//! # Ok(())
//! # }
//! ```

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

// Crate-level exports - Agent configuration
pub use agent_config::{AgentConfig, ConfigError};

// Crate-level exports - Agent library
pub use agent_library::AgentLibrary;

// Crate-level exports - Database
pub use db::{
    AggregatedStats, DbError, GameOutcome, GameRepository, GameStat, NewGameStat, NewUser, User,
};

// Crate-level exports - Lobby
pub use lobby::{LobbyController, Screen, ScreenTransition};

// Crate-level exports - Profile service
pub use profile_service::ProfileService;

// Crate-level exports - Agent handler
pub use agent_handler::GameAgent;

// Crate-level exports - LLM client
pub use llm_client::{LlmClient, LlmConfig, LlmError, LlmProvider};

// Crate-level exports - Server types
pub use server::{
    GameServer, GetBoardRequest, MakeMoveRequest, PlayGameRequest, RegisterPlayerRequest,
};

// Crate-level exports - Session management
pub use session::{GameSession, Player, PlayerType, SessionManager};

// Crate-level exports - TUI
pub use tui::run as run_tui;
pub use tui::run_game_session;
pub use tui::run_lobby;

// Crate-level exports - Game types (tic-tac-toe with typestates)
pub use games::tictactoe::{
    // Domain types
    AnyGame,
    Board,
    Finished,
    // Compatibility alias
    Game,
    GameFinished,
    GameInProgress,
    GameResult,
    // New typestate API (phase-specific structs)
    GameSetup,
    InProgress,
    // Action types
    Move,
    MoveError,
    Outcome,
    Player as TicTacToePlayer,
    Position,
    // Legacy phase markers (deprecated)
    Setup,
    Square,
};
