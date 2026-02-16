//! MCP server setup and configuration.

use crate::games::tictactoe::{Player, Position, ValidPositions};
use crate::session::{PlayerType, SessionManager};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::service::{Peer, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_router, tool_handler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};

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
    /// Position on board.
    pub position: crate::games::tictactoe::Position,
}

/// Request for playing a game with elicitation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlayGameRequest {
    /// Session ID.
    pub session_id: String,
    /// Player name.
    pub player_name: String,
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
            session.game.board().display()
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

        // Reset the game board and clear players for fresh start
        session.game = crate::games::tictactoe::Game::new().into();
        session.player_x = None;
        session.player_o = None;
        self.sessions.update_session(session.clone());
        
        let message = format!("New game started! Players can rejoin.\n{}", session.game.board().display());
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Makes a move at the given position.
    #[instrument(skip(self, req), fields(session_id = %req.session_id, player_id = %req.player_id, position = ?req.position))]
    #[tool(description = "Make a move at the specified position. Use Position enum (TopLeft, TopCenter, TopRight, MiddleLeft, Center, MiddleRight, BottomLeft, BottomCenter, BottomRight).")]
    pub async fn make_move(
        &self,
        Parameters(req): Parameters<MakeMoveRequest>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            session_id = %req.session_id,
            player_id = %req.player_id,
            position = ?req.position,
            "Processing move"
        );

        let mut session = self.sessions.get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        // Make the move (validates turn and position)
        session.make_move(&req.player_id, req.position)
            .map_err(|e| McpError::invalid_params(e, None))?;

        self.sessions.update_session(session.clone());

        let status_msg = session.game.status_string();

        info!(
            session_id = %req.session_id,
            player_id = %req.player_id,
            position = ?req.position,
            status = %status_msg,
            "Move completed successfully"
        );

        let message = format!("{}\n\n{}", status_msg, session.game.board().display());
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

        let player_x_name = session.player_x.as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");
        let player_o_name = session.player_o.as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");

        let current_player_str = session.game.to_move()
            .map(|p| format!("{:?}", p))
            .unwrap_or_else(|| "Game Over".to_string());

        let message = format!(
            "Session: {}\nPlayer X: {}\nPlayer O: {}\nCurrent player: {}\nStatus: {}\nMoves: {}\n\n{}",
            req.session_id,
            player_x_name,
            player_o_name,
            current_player_str,
            session.game.status_string(),
            session.game.history().len(),
            session.game.board().display()
        );
        
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }
    
    /// Lists all available game sessions
    #[instrument(skip(self))]
    #[tool(description = "List all available game sessions to see which ones need players")]
    pub async fn list_sessions(&self) -> Result<CallToolResult, McpError> {
        info!("Listing all game sessions");
        
        let session_ids: Vec<String> = self.sessions.list_sessions();
        
        if session_ids.is_empty() {
            info!("No active sessions found");
            return Ok(CallToolResult::success(vec![Content::text("No active game sessions")]));
        }
        
        let mut result = String::from("Available game sessions:\n\n");
        
        for session_id in &session_ids {
            if let Some(session) = self.sessions.get_session(session_id) {
                let has_x = session.player_x.is_some();
                let has_o = session.player_o.is_some();
                let player_count = if has_x { 1 } else { 0 } + if has_o { 1 } else { 0 };
                let needs_players = player_count < 2;
                let status = if needs_players {
                    format!("⏳ Waiting for {} more player(s)", 2 - player_count)
                } else {
                    "✅ Ready to play".to_string()
                };
                
                result.push_str(&format!(
                    "Session: {}\n  Players: {}/{}\n  Status: {}\n",
                    session_id,
                    player_count,
                    2,
                    status
                ));
                
                // Show player details
                if let Some(px) = &session.player_x {
                    result.push_str(&format!("    - {} (X)\n", px.name));
                }
                if let Some(po) = &session.player_o {
                    result.push_str(&format!("    - {} (O)\n", po.name));
                }
                result.push('\n');
            }
        }
        
        info!(session_count = session_ids.len(), "Listed available sessions");
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Play a game of tic-tac-toe using elicitation
    #[instrument(skip(self, peer, req), fields(session_id = %req.session_id, player_name = %req.player_name))]
    #[tool(description = "Play a complete game of tic-tac-toe. The agent will be prompted for moves interactively until the game ends.")]
    pub async fn play_game(
        &self,
        peer: Peer<RoleServer>,
        Parameters(req): Parameters<PlayGameRequest>,
    ) -> Result<CallToolResult, McpError> {
        
        
        info!(session_id = %req.session_id, player_name = %req.player_name, "Starting elicitation-based game");
        
        // Register the agent player
        let player_id = format!("{}_{}", req.session_id, req.player_name.to_lowercase().replace(' ', "_"));
        
        // Get or create session
        if self.sessions.get_session(&req.session_id).is_none() {
            info!(session_id = %req.session_id, "Creating new session for game");
            self.sessions
                .create_session(req.session_id.clone())
                .map_err(|e: String| McpError::internal_error(e, None))?;
        }
        
        // Register player atomically (thread-safe)
        let mark = self.sessions
            .register_player_atomic(&req.session_id, player_id.clone(), req.player_name.clone(), PlayerType::Agent)
            .map_err(|e| {
                error!(error = %e, "Failed to register player");
                let msg = format!("Failed to register: {}", e);
                McpError::invalid_params(msg, None)
            })?;
        
        info!(player_id = %player_id, mark = ?mark, "Agent registered, entering elicitation loop");
        
        // Game loop - continue until game is over
        loop {
            // Get fresh session state at start of each iteration
            let mut session = self.sessions.get_session(&req.session_id)
                .ok_or_else(|| McpError::internal_error("Session not found", None))?;
            
            // Check if game is over
            if session.game.is_over() {
                if let Some(winner) = session.game.winner() {
                    let winner_name = if winner == mark {
                        req.player_name.clone()
                    } else {
                        "opponent".to_string()
                    };
                    
                    let message = format!(
                        "🎉 Game Over! {} wins!\n\nFinal Board:\n{}\n\nMoves: {}",
                        winner_name,
                        session.game.board().display(),
                        session.game.history().len()
                    );
                    
                    tracing::info!(winner = ?winner, moves = session.game.history().len(), "Game ended with winner");
                    self.sessions.update_session(session);
                    return Ok(CallToolResult::success(vec![Content::text(message)]));
                } else {
                    let message = format!(
                        "🤝 Game Over! It's a draw.\n\nFinal Board:\n{}\n\nMoves: {}",
                        session.game.board().display(),
                        session.game.history().len()
                    );
                    
                    tracing::info!(moves = session.game.history().len(), "Game ended in draw");
                    self.sessions.update_session(session);
                    return Ok(CallToolResult::success(vec![Content::text(message)]));
                }
            }
            
            // Check if it's our turn
            if !session.is_players_turn(&player_id) {
                // Wait for opponent's move (agent vs agent mode)
                tracing::info!(mark = ?mark, "Not our turn, waiting for opponent");
                // Don't update - we haven't modified anything
                
                // Poll for opponent's move
                let max_polls = 300; // 5 minutes (1 second per poll)
                for poll_count in 0..max_polls {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    
                    // Refresh session state
                    let refreshed_session = self.sessions.get_session(&req.session_id)
                        .ok_or_else(|| McpError::internal_error("Session disappeared", None))?;
                    
                    // Check if game ended while we were waiting
                    if refreshed_session.game.is_over() {
                        break; // Exit to outer loop to handle game end
                    }
                    
                    // Check if it's now our turn
                    if refreshed_session.is_players_turn(&player_id) {
                        tracing::info!(poll_count, "Opponent moved, now our turn");
                        break; // Exit poll loop, continue to our move
                    }
                    
                    if poll_count % 10 == 0 {
                        tracing::debug!(poll_count, "Still waiting for opponent");
                    }
                }
                
                // Loop continues to check game status and make our move
                continue;
            }
            
            // It's our turn - use pure elicitation (walled garden pattern)
            tracing::info!(mark = ?mark, "Agent's turn - entering elicitation walled garden");
            
            // Elicitation + Validation loop (demonstrates composition)
            // Elicitation ensures TYPE safety (Position enum)
            // Validation ensures SEMANTIC correctness (square is empty)
            let position = loop {
                // THE ONLY WAY TO GET A POSITION: Through filtered elicitation
                // Server wraps Position::valid_moves into the elicitation call stack
                let candidate = self.elicit_position_filtered(peer.clone(), &req.session_id)
                    .await?;
                
                tracing::info!(position = ?candidate, "Position elicited via framework Select paradigm");
                
                // Validate against board state (composition of elicitation + contracts)
                // Note: With filtering above, this should always pass, but defensive check
                let session = self.sessions.get_session(&req.session_id)
                    .ok_or_else(|| McpError::internal_error("Session disappeared", None))?;
                    
                let board = session.game.board();
                if board.is_empty(candidate) {
                    tracing::debug!(position = ?candidate, "Position validated as empty");
                    break candidate;
                } else {
                    tracing::warn!(
                        position = ?candidate,
                        "Position occupied despite filtering - retrying"
                    );
                    continue;
                }
            };
            
            tracing::info!(position = ?position, "Position elicited via framework Select paradigm");
            
            // Elicitation guarantees type safety, validation loop ensures semantic correctness
            // Session API handles final validation + typestate transitions
            match session.make_move(&player_id, position) {
                Ok(()) => {
                    tracing::info!(position = ?position, "Move executed - typestate transition complete");
                }
                Err(e) => {
                    // Should not happen - we validated above - but defensive
                    tracing::error!(error = %e, position = ?position, "Move rejected despite validation");
                    return Err(McpError::internal_error(
                        format!("Move rejected: {}", e),
                        None,
                    ));
                }
            }
            
            // Update game state atomically (preserves player registrations)
            self.sessions.update_game_atomic(&req.session_id, session.game)
                .map_err(|e| McpError::internal_error(e, None))?;
        }
    }

    /// Elicit a position with board-state filtering (walled garden pattern).
    ///
    /// This demonstrates the pattern for context-aware selection:
    /// 1. Get current board state
    /// 2. Filter Position::ALL to only valid (empty) squares  
    /// 3. Wrap in ValidPositions view struct (derives Elicit)
    /// 4. Call elicitation on the view - framework handles the rest
    ///
    /// Future: This pattern could be generalized with Select::Filter associated type.
    #[instrument(skip(self, peer), fields(session_id))]
    async fn elicit_position_filtered(
        &self,
        peer: Peer<RoleServer>,
        session_id: &str,
    ) -> Result<Position, McpError> {
        // Get current board state for filtering
        let session = self.sessions.get_session(session_id)
            .ok_or_else(|| McpError::internal_error("Session not found", None))?;
        
        let board = session.game.board();
        let valid_positions = Position::valid_moves(board);
        
        if valid_positions.is_empty() {
            return Err(McpError::internal_error("No valid moves available", None));
        }
        
        tracing::debug!(
            valid_count = valid_positions.len(),
            positions = ?valid_positions,
            "Filtered to valid positions"
        );
        
        // Wrap in view struct and elicit through framework
        let view = ValidPositions {
            positions: valid_positions,
        };
        
        let position = view.elicit_position(peer)
            .await
            .map_err(|e| McpError::internal_error(format!("Elicitation failed: {}", e), None))?;
        
        tracing::info!(position = ?position, "Position selected from filtered options");
        Ok(position)
    }

    // Auto-generate elicitation tools for type-safe LLM interaction
    elicitation::elicit_tools! {
        Position,
        Player,
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
