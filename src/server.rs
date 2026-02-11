//! MCP server setup and configuration.

use crate::games::tictactoe::{Game, GameStatus};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_router, tool_handler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, info, instrument};

/// Request for making a move.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MakeMoveRequest {
    /// Position on board (0-8, where 0=top-left, 8=bottom-right).
    pub position: usize,
}

/// Main server handler.
pub struct GameServer {
    game: Arc<Mutex<Game>>,
    tool_router: ToolRouter<Self>,
    /// Channel to send agent moves to orchestrator (optional for TUI mode).
    move_tx: Option<mpsc::UnboundedSender<usize>>,
}

#[tool_router]
impl GameServer {
    /// Creates a new game server.
    pub fn new() -> Self {
        info!("Creating game server");
        let tool_router = Self::tool_router();
        Self {
            game: Arc::new(Mutex::new(Game::new())),
            tool_router,
            move_tx: None,
        }
    }

    /// Creates a new game server with move notification channel.
    /// 
    /// When agent calls make_move, the position is sent via this channel
    /// to the orchestrator for TUI integration.
    pub fn with_move_channel(move_tx: mpsc::UnboundedSender<usize>) -> Self {
        info!("Creating game server with move channel");
        let tool_router = Self::tool_router();
        Self {
            game: Arc::new(Mutex::new(Game::new())),
            tool_router,
            move_tx: Some(move_tx),
        }
    }

    /// Starts a new game.
    #[instrument(skip(self))]
    #[tool(description = "Start a new tic-tac-toe game. Player X goes first.")]
    pub async fn start_game(&self) -> Result<CallToolResult, McpError> {
        info!("Starting new tic-tac-toe game");
        let mut game = self.game.lock().unwrap();
        *game = Game::new();
        
        let message = format!("New game started!\n{}", game.state().board().display());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Makes a move at the given position.
    #[instrument(skip(self, req), fields(position = req.position))]
    #[tool(description = "Make a move at the specified position (0-8). Positions are numbered left-to-right, top-to-bottom.")]
    pub async fn make_move(&self, Parameters(req): Parameters<MakeMoveRequest>) -> Result<CallToolResult, McpError> {
        let position = req.position;
        debug!(position, "Making move");
        let mut game = self.game.lock().unwrap();
        
        let current_player = game.state().current_player();
        
        if let Err(e) = game.make_move(position) {
            return Err(McpError::invalid_params(
                format!("Invalid move: {}", e),
                None,
            ));
        }
        
        let state = game.state();
        let status_msg = match state.status() {
            GameStatus::InProgress => {
                format!("Move accepted. Player {:?} to move.", state.current_player())
            }
            GameStatus::Won(player) => {
                format!("Player {:?} wins!", player)
            }
            GameStatus::Draw => {
                "Game ended in a draw!".to_string()
            }
        };

        info!(
            player = ?current_player,
            position,
            status = ?state.status(),
            "Move completed"
        );

        // Notify orchestrator if channel exists (TUI mode)
        if let Some(tx) = &self.move_tx {
            if let Err(e) = tx.send(position) {
                tracing::warn!(error = %e, "Failed to send move to orchestrator");
            } else {
                debug!(position, "Sent move to orchestrator");
            }
        }

        let message = format!("{}\n\n{}", status_msg, state.board().display());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Gets the current board state.
    #[instrument(skip(self))]
    #[tool(description = "Get the current board state and game status")]
    pub async fn get_board(&self) -> Result<CallToolResult, McpError> {
        debug!("Getting board state");
        let game = self.game.lock().unwrap();
        let state = game.state();

        let message = format!(
            "Current player: {:?}\nStatus: {:?}\nMoves: {}\n\n{}",
            state.current_player(),
            state.status(),
            state.history().len(),
            state.board().display()
        );
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GameServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Type-safe tic-tac-toe game server".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
