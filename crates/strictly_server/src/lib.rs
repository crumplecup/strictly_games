//! Strictly Server - Game orchestration and multi-frontend rendering
//!
//! All three games (Tic-tac-toe, Blackjack, Craps) share a single
//! WCAG-verified [AccessKit](https://accesskit.dev) IR pipeline and are
//! delivered through three parallel frontends:
//!
//! | Frontend | Entry point | Transport |
//! |----------|-------------|-----------|
//! | ratatui TUI | [`tui_run`] | Terminal |
//! | egui native window | [`run_egui`] | wgpu / winit |
//! | leptos HTTP | [`run_leptos`] | axum / HTML |
//!
//! Every render path follows the same proof chain:
//! `*_to_verified_tree() → WcagVerified → RenderComplete → *UiConsistent`
//!
//! This library also provides:
//! - **MCP server** for LLM agent integration
//! - **REST API** for game operations
//! - **Database** persistence for users and stats
//! - **Session management** for multiplayer games
//! - **Lobby system** with TUI
//!
//! Uses `strictly_tictactoe` for pure game logic.

#![warn(missing_docs)]
#![forbid(unsafe_code)]
// The multi-player blackjack async state machine is deeply nested; 128 is
// insufficient for the layout computation of the controller's run() future.
#![recursion_limit = "256"]

// Private module declarations
mod agent_config;
mod agent_handler;
mod agent_library;
mod db;
mod egui_frontend;
mod games;
mod leptos_frontend;
mod llm_client;
mod lobby;
mod profile_service;
mod server;
mod session;
mod tui;
mod verify;

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
pub use lobby::{
    FirstPlayer, GameType, LobbyController, LobbySettings, PlayerKind, PlayerSlot, Screen,
    ScreenTransition,
};

// Public API exports - Profile service
pub use profile_service::ProfileService;

// Public API exports - LLM client
pub use llm_client::{LlmClient, LlmConfig, LlmError, LlmProvider, ToolSpec};

// Public API exports - Server
pub use server::GameServer;

// Public API exports - Session
pub use session::{
    DialogueEntry, ExploreStats, GameSession, SeatEntry, SessionManager, SharedTable,
    SharedTablePhase, SharedTableSeatView, SharedTableState, new_shared_table,
};

// Public API exports - TUI
pub use tui::{run as tui_run, run_blackjack_mcp_session, run_game_session};

// Public API exports - Egui frontend
pub use egui_frontend::run_egui;

// Public API exports - Leptos frontend
pub use leptos_frontend::{
    LeptosAppState, leptos_game_router, render_bj_html, render_craps_html, render_ttt_html,
    run_leptos,
};

// Public API exports - Verification
pub use verify::{run_all as run_verification_all, run_creusot, run_kani, run_verus};

// Public API exports - TicTacToe types
pub use games::tictactoe::{
    AnyGame, Board, GameFinished, GameInProgress, GameResult, GameSetup, Mark, Move, MoveError,
    Outcome, Player, Position, Square,
};

// Public API exports - TicTacToe contract propositions
pub use games::tictactoe::{PlayerTurn, SquareEmpty};

// Public API exports - Blackjack types (game logic from strictly_blackjack, workflow from server)
pub use games::blackjack::{BlackjackStateView, BlackjackWorkflow, HandResult};
pub use strictly_blackjack::{
    ActionError, BasicAction, BetPlaced, GameBetting, GameDealerTurn,
    GameFinished as BlackjackFinished, GamePlayerTurn, GameResult as BlackjackResult,
    GameSetup as BlackjackSetup, PayoutSettled, PlayerAction, PlayerTurnComplete,
};

// Re-export for convenience
pub use strictly_tictactoe;

// Type aliases for compatibility
/// Player type for TUI
pub type TicTacToePlayer = Player;
