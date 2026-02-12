//! MCP server setup and configuration.

use crate::games::tictactoe::GameStatus;
use crate::session::{PlayerType, SessionManager};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_router, tool_handler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

/// Request for registering a player.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegisterPlayerRequest {
    /// Session ID to join.
    pub session_id: String,
    /// Player name.
    pub name: String,
    /// Player type (human or agent).
    #[serde(rename = "type")]
    pub player_type: PlayerType,
}

/// Request for making a move.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MakeMoveRequest {
    /// Session ID.
    pub session_id: String,
    /// Player ID.
    pub player_id: String,
    /// Position on board (0-8, where 0=top-left, 8=bottom-right).
    pub position: usize,
}

/// Request for getting board state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetBoardRequest {
    /// Session ID.
    pub session_id: String,
}

/// Main server handler.
pub struct GameServer {
    sessions: SessionManager,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl GameServer {
    /// Creates a new game server with shared session manager.
    #[instrument]
    pub fn with_sessions(sessions: SessionManager) -> Self {
        info!("Creating game server with shared session manager");
        Self {
            sessions,
            tool_router: Self::tool_router(),
        }
    }

    /// Creates a new game server.
    pub fn new() -> Self {
        info!("Creating game server with session management");
        Self::with_sessions(SessionManager::new())
    }

    /// Registers a player in a session.
    #[instrument(skip(self, req), fields(session_id = %req.session_id, name = %req.name))]
    #[tool(description = "Register as a player in a game session. Creates session if it doesn't exist.")]
    pub async fn register_player(
        &self,
        Parameters(req): Parameters<RegisterPlayerRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(
            session_id = %req.session_id,
            name = %req.name,
            player_type = ?req.player_type,
            "Registering player"
        );

        // Create session if it doesn't exist
        if self.sessions.get_session(&req.session_id).is_none() {
            info!(session_id = %req.session_id, "Creating new session");
            self.sessions
                .create_session(req.session_id.clone())
                .map_err(|e| McpError::internal_error(e, None))?;
        }

        // Get session and register player
        let mut session = self.sessions.get_session(&req.session_id)
            .ok_or_else(|| McpError::internal_error("Session not found after creation", None))?;

        // Generate player ID
        let player_id = format!("{}_{}", req.session_id, req.name.to_lowercase().replace(' ', "_"));
        
        let mark = session
            .register_player(player_id.clone(), req.name.clone(), req.player_type)
            .map_err(|e| McpError::invalid_params(e, None))?;

        self.sessions.update_session(session.clone());

        let message = format!(
            "Registered as player {:?}!\nPlayer ID: {}\nSession: {}\n\n{}",
            mark,
            player_id,
            req.session_id,
            session.game.state().board().display()
        );

        info!(
            session_id = %req.session_id,
            player_id = %player_id,
            mark = ?mark,
            "Player registered successfully"
        );

        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Starts a new game in a session.
    #[instrument(skip(self, req), fields(session_id = %req.session_id))]
    #[tool(description = "Start a new tic-tac-toe game in the session. Player X goes first.")]
    pub async fn start_game(
        &self,
        Parameters(req): Parameters<GetBoardRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(session_id = %req.session_id, "Starting new game");
        
        let mut session = self.sessions.get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found. Use register_player first.", None))?;

        session.game = crate::games::tictactoe::Game::new();
        self.sessions.update_session(session.clone());
        
        let message = format!("New game started!\n{}", session.game.state().board().display());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Makes a move at the given position.
    #[instrument(skip(self, req), fields(session_id = %req.session_id, player_id = %req.player_id, position = req.position))]
    #[tool(description = "Make a move at the specified position (0-8). Positions are numbered left-to-right, top-to-bottom.")]
    pub async fn make_move(
        &self,
        Parameters(req): Parameters<MakeMoveRequest>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            session_id = %req.session_id,
            player_id = %req.player_id,
            position = req.position,
            "Processing move"
        );

        let mut session = self.sessions.get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        // Make the move (validates turn and position)
        session.make_move(&req.player_id, req.position)
            .map_err(|e| McpError::invalid_params(e, None))?;

        self.sessions.update_session(session.clone());

        let state = session.game.state();
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
            session_id = %req.session_id,
            player_id = %req.player_id,
            position = req.position,
            status = ?state.status(),
            "Move completed successfully"
        );

        let message = format!("{}\n\n{}", status_msg, state.board().display());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Gets the current board state.
    #[instrument(skip(self, req), fields(session_id = %req.session_id))]
    #[tool(description = "Get the current board state and game status")]
    pub async fn get_board(
        &self,
        Parameters(req): Parameters<GetBoardRequest>,
    ) -> Result<CallToolResult, McpError> {
        debug!(session_id = %req.session_id, "Getting board state");
        
        let session = self.sessions.get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        let state = session.game.state();

        let player_x_name = session.player_x.as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");
        let player_o_name = session.player_o.as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");

        let message = format!(
            "Session: {}\nPlayer X: {}\nPlayer O: {}\nCurrent player: {:?}\nStatus: {:?}\nMoves: {}\n\n{}",
            req.session_id,
            player_x_name,
            player_o_name,
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
