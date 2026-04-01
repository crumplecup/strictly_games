//! MCP server setup and configuration.

use crate::games::tictactoe::{Player, Position};
use crate::session::{PlayerType, SessionManager};
use elicitation::{ChoiceSet, ElicitServer, Elicitation};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::service::{Peer, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strictly_blackjack::{BasicAction, GameSetup};
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

/// Request for playing blackjack with elicitation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlayBlackjackRequest {
    /// Initial bankroll for the player.
    pub initial_bankroll: u64,
}

/// Request for getting board state.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetBoardRequest {
    /// Session ID.
    pub session_id: String,
}

/// Request for cancelling a game.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancelGameRequest {
    /// Session ID.
    pub session_id: String,
}

/// Main server handler.
pub struct GameServer {
    sessions: SessionManager,
    tool_router: ToolRouter<Self>,
}

impl Default for GameServer {
    #[instrument]
    fn default() -> Self {
        Self::new()
    }
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
    #[instrument]
    pub fn new() -> Self {
        info!("Creating game server with session management");
        Self::with_sessions(SessionManager::new())
    }

    /// Registers a player in a session.
    #[instrument(skip(self, req), fields(session_id = %req.session_id, name = %req.name))]
    #[tool(
        description = "Register as a player in a game session. Creates session if it doesn't exist."
    )]
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
        let mut session = self
            .sessions
            .get_session(&req.session_id)
            .ok_or_else(|| McpError::internal_error("Session not found after creation", None))?;

        // Generate player ID
        let player_id = format!(
            "{}_{}",
            req.session_id,
            req.name.to_lowercase().replace(' ', "_")
        );

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

        let mut session = self.sessions.get_session(&req.session_id).ok_or_else(|| {
            McpError::invalid_params("Session not found. Use register_player first.", None)
        })?;

        // Reset the game board and clear players for fresh start
        session.game = crate::games::tictactoe::Game::new().into();
        session.player_x = None;
        session.player_o = None;
        self.sessions.update_session(session.clone());

        let message = format!(
            "New game started! Players can rejoin.\n{}",
            session.game.board().display()
        );
        Ok(CallToolResult::success(vec![Content::text(message)]))
    }

    /// Makes a move at the given position.
    #[instrument(skip(self, req), fields(session_id = %req.session_id, player_id = %req.player_id, position = ?req.position))]
    #[tool(
        description = "Make a move at the specified position. Use Position enum (TopLeft, TopCenter, TopRight, MiddleLeft, Center, MiddleRight, BottomLeft, BottomCenter, BottomRight)."
    )]
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

        let mut session = self
            .sessions
            .get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        // Make the move (validates turn and position)
        session
            .make_move(&req.player_id, req.position)
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

        let session = self
            .sessions
            .get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        let player_x_name = session
            .player_x
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");
        let player_o_name = session
            .player_o
            .as_ref()
            .map(|p| p.name.as_str())
            .unwrap_or("(waiting)");

        let current_player_str = session
            .game
            .to_move()
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

    /// Cancels an ongoing game (triggers passive-Affirm escape hatch).
    #[instrument(skip(self, req), fields(session_id = %req.session_id))]
    #[tool(description = "Cancel the current game (allows graceful exit from game loop)")]
    pub async fn cancel_game(
        &self,
        Parameters(req): Parameters<CancelGameRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(session_id = %req.session_id, "Cancelling game via escape hatch");

        let session = self
            .sessions
            .get_session(&req.session_id)
            .ok_or_else(|| McpError::invalid_params("Session not found", None))?;

        session.request_cancel();

        Ok(CallToolResult::success(vec![Content::text(
            "Game cancellation requested.".to_string(),
        )]))
    }

    /// Lists all available game sessions
    #[instrument(skip(self))]
    #[tool(description = "List all available game sessions to see which ones need players")]
    pub async fn list_sessions(&self) -> Result<CallToolResult, McpError> {
        info!("Listing all game sessions");

        let session_ids: Vec<String> = self.sessions.list_sessions();

        if session_ids.is_empty() {
            info!("No active sessions found");
            return Ok(CallToolResult::success(vec![Content::text(
                "No active game sessions",
            )]));
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
                    session_id, player_count, 2, status
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

        info!(
            session_count = session_ids.len(),
            "Listed available sessions"
        );
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    /// Play a game of tic-tac-toe using elicitation
    #[instrument(skip(self, peer, req), fields(session_id = %req.session_id, player_name = %req.player_name))]
    #[tool(
        description = "Play a complete game of tic-tac-toe. The agent will be prompted for moves interactively until the game ends."
    )]
    pub async fn play_game(
        &self,
        peer: Peer<RoleServer>,
        Parameters(req): Parameters<PlayGameRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(session_id = %req.session_id, player_name = %req.player_name, "Starting elicitation-based game");

        // Register the agent player
        let player_id = format!(
            "{}_{}",
            req.session_id,
            req.player_name.to_lowercase().replace(' ', "_")
        );

        // Get or create session
        if self.sessions.get_session(&req.session_id).is_none() {
            info!(session_id = %req.session_id, "Creating new session for game");
            self.sessions
                .create_session(req.session_id.clone())
                .map_err(|e: String| McpError::internal_error(e, None))?;
        }

        // Register player atomically (thread-safe)
        let mark = self
            .sessions
            .register_player_atomic(
                &req.session_id,
                player_id.clone(),
                req.player_name.clone(),
                PlayerType::Agent,
            )
            .map_err(|e| {
                error!(error = %e, "Failed to register player");
                let msg = format!("Failed to register: {}", e);
                McpError::invalid_params(msg, None)
            })?;

        info!(player_id = %player_id, mark = ?mark, "Agent registered, entering elicitation loop");

        // Game loop - continue until game is over
        loop {
            // PASSIVE-AFFIRM: Escape hatch check (no user prompt, just flag)
            // This is the building block for control flow - user can press 'q' to cancel
            let session = self
                .sessions
                .get_session(&req.session_id)
                .ok_or_else(|| McpError::internal_error("Session not found", None))?;

            if !session.affirm_continue() {
                info!("Game loop cancelled by user request (passive-affirm escape hatch)");
                return Ok(CallToolResult::success(vec![Content::text(
                    "Game cancelled by user.".to_string(),
                )]));
            }

            // Get fresh session state at start of each iteration
            let mut session = self
                .sessions
                .get_session(&req.session_id)
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

                    // PASSIVE-AFFIRM: Check escape hatch while waiting
                    let check_session = self
                        .sessions
                        .get_session(&req.session_id)
                        .ok_or_else(|| McpError::internal_error("Session disappeared", None))?;

                    if !check_session.affirm_continue() {
                        info!("Game loop cancelled while waiting for opponent");
                        return Ok(CallToolResult::success(vec![Content::text(
                            "Game cancelled by user.".to_string(),
                        )]));
                    }

                    // Refresh session state
                    let refreshed_session = self
                        .sessions
                        .get_session(&req.session_id)
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
                let candidate = self
                    .elicit_position_filtered(peer.clone(), &req.session_id)
                    .await?;

                tracing::info!(position = ?candidate, "Position elicited via framework Select paradigm");

                // Validate against board state (composition of elicitation + contracts)
                // Note: With filtering above, this should always pass, but defensive check
                let session = self
                    .sessions
                    .get_session(&req.session_id)
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
            self.sessions
                .update_game_atomic(&req.session_id, session.game)
                .map_err(|e| McpError::internal_error(e, None))?;
        }
    }

    /// Elicit a position with board-state filtering and agent exploration.
    ///
    /// Agents see both commit (Play) and explore (ViewBoard, ViewLegalMoves,
    /// ViewThreats) variants via [`TicTacToeAction`]. Explore selections
    /// build a [`TicTacToeView`] snapshot, send the description through the
    /// communicator, and re-elicit. When the agent selects Play(pos) the
    /// position is returned.
    #[instrument(skip(self, peer), fields(session_id))]
    async fn elicit_position_filtered(
        &self,
        peer: Peer<RoleServer>,
        session_id: &str,
    ) -> Result<Position, McpError> {
        use crate::session::DialogueEntry;
        use crate::tui::contextual_communicator::{ContextualCommunicator, knowledge_cache};
        use elicitation::ElicitCommunicator as _;
        use strictly_tictactoe::{TicTacToeAction, TicTacToeView};

        let knowledge = knowledge_cache();
        let comm = ContextualCommunicator::new(ElicitServer::new(peer), knowledge.clone());

        self.sessions.push_dialogue(
            session_id,
            DialogueEntry::server("Eliciting move — select a position or explore."),
        );

        loop {
            let action = TicTacToeAction::elicit(&comm)
                .await
                .map_err(|e| McpError::internal_error(format!("Elicitation failed: {e}"), None))?;

            if let Some(pos) = action.to_position() {
                // Commit — validate the position is still legal
                let session = self
                    .sessions
                    .get_session(session_id)
                    .ok_or_else(|| McpError::internal_error("Session not found", None))?;

                let board = session.game.board();
                if board.is_empty(pos) {
                    self.sessions.record_play(session_id);
                    self.sessions.push_dialogue(
                        session_id,
                        DialogueEntry::agent(format!("Play {}", pos.label())),
                    );
                    tracing::info!(position = ?pos, "Position elicited and validated");
                    return Ok(pos);
                }
                // Occupied despite agent choice — inform and retry
                self.sessions.push_dialogue(
                    session_id,
                    DialogueEntry::agent(format!("Play {} (invalid)", pos.label())),
                );
                let rejection = format!(
                    "[Invalid Move] Position {} is occupied. Please choose an empty square.",
                    pos
                );
                self.sessions
                    .push_dialogue(session_id, DialogueEntry::server(&rejection));
                knowledge
                    .lock()
                    .unwrap()
                    .push(format!("[Invalid Move] Position {} is occupied.", pos));
                let _ = comm.send_prompt(&rejection).await;
                continue;
            }

            // Explore — record, build view, cache knowledge, and loop
            self.sessions.record_explore(session_id);
            let category = action.explore_category().unwrap_or("unknown");
            self.sessions.push_dialogue(
                session_id,
                DialogueEntry::agent(format!("Explore: {category}")),
            );

            let session = self
                .sessions
                .get_session(session_id)
                .ok_or_else(|| McpError::internal_error("Session not found", None))?;

            let current_player = session
                .game
                .to_move()
                .ok_or_else(|| McpError::internal_error("No player to move", None))?;

            let view = TicTacToeView::from_board(session.game.board(), current_player);
            let description = view
                .describe_category(category)
                .unwrap_or_else(|| "No information available".to_string());

            tracing::debug!(category, "Agent exploring game state");

            // Record the server's response to the explore request.
            self.sessions.push_dialogue(
                session_id,
                DialogueEntry::server(format!("[{category}] {description}")),
            );

            // Add to the growing knowledge cache so the agent sees it
            // in every subsequent prompt until it commits.
            knowledge
                .lock()
                .unwrap()
                .push(format!("[{category}] {description}"));

            let _ = comm
                .send_prompt(&format!("[Game State — {category}] {description}"))
                .await;
        }
    }

    /// Play a complete game of blackjack with elicitation.
    #[instrument(skip(self, peer, req), fields(initial_bankroll = %req.initial_bankroll))]
    #[tool(
        description = "Play blackjack. You will be prompted for betting and actions (hit/stand) until the game ends."
    )]
    pub async fn play_blackjack(
        &self,
        peer: Peer<RoleServer>,
        Parameters(req): Parameters<PlayBlackjackRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(initial_bankroll = %req.initial_bankroll, "Starting blackjack game");

        // Initialize game
        let mut bankroll = req.initial_bankroll;
        let mut result_messages = Vec::new();

        // Game loop - play hands until bankroll is depleted or player quits
        loop {
            // Check if player has funds
            if bankroll == 0 {
                let message = format!(
                    "💸 Game Over! You've run out of chips.\n\nFinal bankroll: ${}\n\n",
                    bankroll
                );
                result_messages.push(message);
                break;
            }

            // Display current bankroll
            result_messages.push(format!("\n💰 Current bankroll: ${}\n", bankroll));

            // Create new betting state
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(42);
            let game = GameSetup::new(seed).start_betting(bankroll);

            // Elicit bet amount
            let bet = self.elicit_bet(peer.clone(), bankroll).await?;

            result_messages.push(format!("Bet: ${}\n", bet));

            // Place bet and deal cards
            let mut game_result = game
                .place_bet(bet)
                .map_err(|e| McpError::invalid_params(format!("Invalid bet: {}", e), None))?;

            // Show initial deal
            match &game_result {
                strictly_blackjack::GameResult::PlayerTurn(player_game) => {
                    result_messages.push(format!(
                        "Your hand: {}\n",
                        player_game.player_hands()[0].display()
                    ));
                    result_messages.push(format!(
                        "Dealer shows: {}\n",
                        player_game.dealer_hand().cards()[0]
                    ));
                }
                strictly_blackjack::GameResult::Finished(finished, _settled) => {
                    // Immediate blackjack
                    result_messages.push(format!(
                        "🃏 Blackjack! Your hand: {}\n",
                        finished.player_hands()[0].display()
                    ));
                    result_messages.push(self.format_game_result(finished));
                    bankroll = finished.bankroll();
                    continue;
                }
                _ => {}
            }

            // Player turn - elicit actions
            while let strictly_blackjack::GameResult::PlayerTurn(player_game) = game_result {
                let action = self
                    .elicit_blackjack_action(peer.clone(), &player_game)
                    .await?;

                result_messages.push(format!("Action: {:?}\n", action));

                let player_action =
                    strictly_blackjack::PlayerAction::new(action, player_game.current_hand_index());

                game_result = player_game
                    .take_action(player_action)
                    .map_err(|e| McpError::internal_error(format!("Action failed: {}", e), None))?;

                // Show updated hand after action
                if let strictly_blackjack::GameResult::PlayerTurn(pg) = &game_result {
                    result_messages.push(format!(
                        "Your hand: {}\n",
                        pg.player_hands()[pg.current_hand_index()].display()
                    ));
                }
            }

            // Dealer turn
            if let strictly_blackjack::GameResult::DealerTurn(dealer_game) = game_result {
                result_messages.push("\n🎲 Dealer's turn...\n".to_string());
                let (finished, _settled) = dealer_game.play_dealer_turn();
                result_messages.push(self.format_game_result(&finished));
                bankroll = finished.bankroll();
            }

            // Ask if player wants to continue
            if !self.affirm_continue_blackjack(peer.clone()).await? {
                result_messages.push("\n👋 Thanks for playing!\n".to_string());
                break;
            }
        }

        let final_message = result_messages.join("");
        Ok(CallToolResult::success(vec![Content::text(final_message)]))
    }

    /// Elicit a bet amount within bankroll limits using ChoiceSet.
    ///
    /// Presents fixed bet denominations filtered by available bankroll.
    #[instrument(skip(self, peer), fields(max_bet))]
    async fn elicit_bet(&self, peer: Peer<RoleServer>, max_bet: u64) -> Result<u64, McpError> {
        tracing::info!(max_bet, "Eliciting bet amount from agent");

        // Fixed bet denominations ($1, $5, $10, $25, $50, $100, $500)
        let bet_options = vec![1, 5, 10, 25, 50, 100, 500];

        // Filter to only bets within bankroll (walled garden)
        let valid_bets: Vec<u64> = bet_options
            .into_iter()
            .filter(|&bet| bet <= max_bet)
            .collect();

        if valid_bets.is_empty() {
            return Err(McpError::internal_error("No valid bets available", None));
        }

        // Use ChoiceSet to trap agent in valid options
        let server = ElicitServer::new(peer);
        let bet = ChoiceSet::new(valid_bets)
            .with_prompt("Choose your bet amount:")
            .elicit(&server)
            .await
            .map_err(|e| McpError::internal_error(format!("Elicitation failed: {}", e), None))?;

        tracing::info!(bet, "Agent selected bet");
        Ok(bet)
    }

    /// Elicit a blackjack action (hit/stand) based on game state using ChoiceSet.
    ///
    /// Uses the walled garden pattern - only presents valid actions to the agent.
    /// Agent cannot choose invalid actions.
    #[instrument(skip(self, peer, game))]
    async fn elicit_blackjack_action(
        &self,
        peer: Peer<RoleServer>,
        game: &strictly_blackjack::GamePlayerTurn,
    ) -> Result<BasicAction, McpError> {
        let hand = &game.player_hands()[game.current_hand_index()];
        let hand_value = hand.value().best();

        tracing::info!(hand_value, "Eliciting action from agent");

        // Walled garden: Filter valid actions based on game state
        // For Milestone 1 (Hit/Stand only), both are always valid unless bust
        let valid_actions: Vec<BasicAction> = if hand.is_bust() {
            // Hand is bust - no actions available (this shouldn't happen in practice)
            vec![]
        } else {
            // Both Hit and Stand are valid
            vec![BasicAction::Hit, BasicAction::Stand]
        };

        if valid_actions.is_empty() {
            return Err(McpError::internal_error("No valid actions available", None));
        }

        // Use ChoiceSet to trap agent in valid options
        let server = ElicitServer::new(peer);
        let action = ChoiceSet::new(valid_actions)
            .with_prompt(format!(
                "Your hand: {} (value: {}). Choose your action:",
                hand.display(),
                hand_value
            ))
            .elicit(&server)
            .await
            .map_err(|e| McpError::internal_error(format!("Elicitation failed: {}", e), None))?;

        tracing::info!(action = ?action, hand_value, "Agent selected action");
        Ok(action)
    }

    /// Ask if player wants to continue playing using bool::elicit_checked (Affirm pattern).
    #[instrument(skip(self, peer))]
    async fn affirm_continue_blackjack(&self, peer: Peer<RoleServer>) -> Result<bool, McpError> {
        tracing::info!("Eliciting continue decision from agent");

        // Use Affirm pattern for yes/no question
        let continue_playing = bool::elicit_checked(peer)
            .await
            .map_err(|e| McpError::internal_error(format!("Elicitation failed: {}", e), None))?;

        tracing::info!(continue_playing, "Agent decision");
        Ok(continue_playing)
    }

    /// Format game result for display.
    fn format_game_result(&self, game: &strictly_blackjack::GameFinished) -> String {
        let mut result = String::new();

        result.push_str(&format!(
            "Dealer's hand: {}\n",
            game.dealer_hand().display()
        ));

        for (i, (hand, outcome)) in game.player_hands().iter().zip(game.outcomes()).enumerate() {
            result.push_str(&format!("\nHand {}: {}\n", i + 1, hand.display()));
            result.push_str(&format!("Outcome: {}\n", outcome));

            let payout = outcome.calculate_payout(game.bets()[i]);
            if payout > 0 {
                result.push_str(&format!("Won: ${}\n", payout));
            } else if payout < 0 {
                result.push_str(&format!("Lost: ${}\n", payout.abs()));
            } else {
                result.push_str("Push\n");
            }
        }

        result.push_str(&format!("\n💰 Bankroll: ${}\n", game.bankroll()));
        result
    }

    // Auto-generate elicitation tools for type-safe LLM interaction
    elicitation::elicit_tools! {
        Position,
        Player,
        BasicAction,
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GameServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerInfo::new(capabilities).with_instructions("Type-safe tic-tac-toe game server")
    }
}
