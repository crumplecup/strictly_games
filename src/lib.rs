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
//! );
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![forbid(unsafe_code)]

// Private module declarations
mod agent_config;
mod agent_handler;
mod elicit_router;
mod games;
mod llm_client;
mod server;
mod session;
mod tui;

// Crate-level exports - Agent configuration
pub use agent_config::{AgentConfig, ConfigError};

// Crate-level exports - Agent handler
pub use agent_handler::GameAgent;

// Crate-level exports - Elicitation router
pub use elicit_router::TicTacToeElicitRouter;

// Crate-level exports - LLM client
pub use llm_client::{LlmClient, LlmConfig, LlmError, LlmProvider};

// Crate-level exports - Server types
pub use server::{GameServer, GetBoardRequest, MakeMoveRequest, PlayGameRequest, RegisterPlayerRequest};

// Crate-level exports - Session management
pub use session::{GameSession, Player, PlayerType, SessionManager};

// Crate-level exports - TUI
pub use tui::run as run_tui;

// Crate-level exports - Game types (tic-tac-toe with typestates)
pub use games::tictactoe::{
    // New typestate API (phase-specific structs)
    GameSetup, GameInProgress, GameFinished, GameResult,
    // Compatibility alias
    Game,
    // Action types
    Move, MoveError,
    // Legacy phase markers (deprecated)
    Setup, InProgress, Finished, Outcome,
    // Domain types
    AnyGame, Board, Position, Square,
    Player as TicTacToePlayer,
};
