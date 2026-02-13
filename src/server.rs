//! MCP server setup and configuration.

use crate::games::tictactoe::GameStatus;
use crate::session::{PlayerType, SessionManager};
use elicitation::Elicitation;
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
    /// Position on board (0-8, where 0=top-left, 8=bottom-right).
    pub position: usize,
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
            
            let game_state = session.game.state();
            
            // Check if game is over
            match game_state.status() {
                GameStatus::Won(winner) => {
                    let winner_name = if winner == &mark {
                        req.player_name.clone()
                    } else {
                        "opponent".to_string()
                    };
                    
                    let message = format!(
                        "🎉 Game Over! {} wins!\n\nFinal Board:\n{}\n\nMoves: {}",
                        winner_name,
                        game_state.board().display(),
                        game_state.history().len()
                    );
                    
                    info!(winner = ?winner, moves = game_state.history().len(), "Game ended with winner");
                    self.sessions.update_session(session);
                    return Ok(CallToolResult::success(vec![Content::text(message)]));
                }
                GameStatus::Draw => {
                    let message = format!(
                        "🤝 Game Over! It's a draw.\n\nFinal Board:\n{}\n\nMoves: {}",
                        game_state.board().display(),
                        game_state.history().len()
                    );
                    
                    info!(moves = game_state.history().len(), "Game ended in draw");
                    self.sessions.update_session(session);
                    return Ok(CallToolResult::success(vec![Content::text(message)]));
                }
                GameStatus::InProgress => {
                    // Check whose turn it is
                    if game_state.current_player() != mark {
                        // Wait for opponent's move (agent vs agent mode)
                        info!(mark = ?mark, "Not our turn, waiting for opponent");
                        self.sessions.update_session(session);
                        
                        // Poll for opponent's move
                        let max_polls = 300; // 5 minutes (1 second per poll)
                        for poll_count in 0..max_polls {
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            
                            // Refresh session state
                            let refreshed_session = self.sessions.get_session(&req.session_id)
                                .ok_or_else(|| McpError::internal_error("Session disappeared", None))?;
                            
                            let updated_state = refreshed_session.game.state();
                            
                            // Check if game ended while we were waiting
                            if !matches!(updated_state.status(), GameStatus::InProgress) {
                                break; // Exit to outer loop to handle game end
                            }
                            
                            // Check if it's now our turn
                            if updated_state.current_player() == mark {
                                info!(poll_count, "Opponent moved, now our turn");
                                break; // Exit poll loop, continue to our move
                            }
                            
                            if poll_count % 10 == 0 {
                                debug!(poll_count, "Still waiting for opponent");
                            }
                        }
                        
                        // Loop continues to check game status and make our move
                        continue;
                    }
                    
                    // It's our turn - use elicitation to get validated move
                    info!(mark = ?mark, "Agent's turn, eliciting move with contracts");
                    
                    // Create ElicitServer wrapper for MCP peer
                    let elicit_server = elicitation::ElicitServer::new(peer.clone());
                    
                    // Retry loop for move elicitation + validation
                    let max_retries = 5;
                    let mut attempt = 0;
                    let validated_move = loop {
                        attempt += 1;
                        
                        // Elicit move from agent using Elicitation framework
                        let mv = <crate::games::tictactoe::Move as Elicitation>::elicit(&elicit_server)
                            .await
                            .map_err(|e| {
                                error!(error = ?e, attempt, "Elicitation failed");
                                let msg = format!("Failed to elicit move: {}", e);
                                McpError::internal_error(msg, None)
                            })?;
                        
                        info!(position = mv.position, attempt, "Agent proposed move via elicitation");
                        
                        // Validate move and get proof of legality
                        match crate::games::tictactoe::validate_move(
                            game_state,
                            &mv,
                            mark,
                        ) {
                            Ok(proof) => {
                                info!(position = mv.position, attempt, "Move validated with proof");
                                break (mv, proof);
                            }
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    position = mv.position,
                                    attempt,
                                    max_retries,
                                    "Move validation failed, will retry"
                                );
                                
                                if attempt >= max_retries {
                                    error!(
                                        position = mv.position,
                                        attempts = attempt,
                                        "Max retries exceeded for move validation"
                                    );
                                    let msg = format!(
                                        "Move validation failed after {} attempts. Last error: {}",
                                        attempt, e
                                    );
                                    return Err(McpError::invalid_params(msg, None));
                                }
                                
                                // Loop continues to retry elicitation
                            }
                        }
                    };
                    
                    let (mv, proof) = validated_move;
                    info!(position = mv.position, "Executing validated move");
                    
                    // Execute move with proof (compile-time guarantee it's legal)
                    let _move_made = crate::games::tictactoe::execute_move(
                        &mut session.game,
                        &mv,
                        mark,
                        proof,
                    );
                    
                    info!(position = mv.position, "Move executed successfully, continuing loop");
                    
                    // Update session and continue loop
                    self.sessions.update_session(session.clone());
                    
                    // Reload session for next iteration
                    session = self.sessions.get_session(&req.session_id)
                        .ok_or_else(|| McpError::internal_error("Session disappeared", None))?;
                }
            }
        }
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
