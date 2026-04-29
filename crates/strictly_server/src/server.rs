//! MCP server setup and configuration.

use crate::games::blackjack::{BetConstraints, DEFAULT_PRESETS, register_bet_tools};
use crate::games::tictactoe::{TttGameContext, register_await_turn_tool, register_move_tools};
use crate::session::{DialogueEntry, PlayerType, SessionManager};
use elicitation::{DynamicToolRegistry, ElicitPlugin as _, TypeSpecPlugin};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo,
};
use rmcp::service::{Peer, RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
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
    /// Session ID for state observation (e.g. `"human_bj"`).
    ///
    /// When provided, the session manager tracks this seat's index so the TUI
    /// can poll `/api/sessions/{id}/blackjack_state`.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Initial bankroll for the player.
    pub initial_bankroll: u64,
    /// Total number of seats at the table (first caller sets this; others ignored).
    ///
    /// Defaults to 1 if not supplied.
    #[serde(default = "default_num_seats")]
    pub num_seats: u64,
    /// Display name for this player (shown in TUI panel title).
    #[serde(default)]
    pub player_name: Option<String>,
}

fn default_num_seats() -> u64 {
    1
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
    /// Dynamic tool registry — holds betting, action, and ttt move tools.
    dynamic: DynamicToolRegistry,
    /// Seat index for this connection's blackjack seat.
    ///
    /// Set once when `blackjack_deal` is called; never changes after that.
    seat_index: Arc<OnceLock<usize>>,
    /// Session ID for this connection — set once when `blackjack_deal` or
    /// `play_game` is called, used to log dialogue entries.
    session_id: Arc<OnceLock<String>>,
    /// Type-spec plugin — exposes `type_spec__describe_type` and
    /// `type_spec__explore_type` so agents can query game-type contracts.
    type_spec: TypeSpecPlugin,
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
            dynamic: DynamicToolRegistry::new(),
            seat_index: Arc::new(OnceLock::new()),
            session_id: Arc::new(OnceLock::new()),
            type_spec: TypeSpecPlugin::new(),
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
        description = "Join a tic-tac-toe session as an agent player. Returns immediately with the current board state and either the first set of move tools (`ttt__*`) if it is your turn, or `ttt__await_turn` if you must wait for the opponent."
    )]
    pub async fn play_game(
        &self,
        peer: Peer<RoleServer>,
        Parameters(req): Parameters<PlayGameRequest>,
    ) -> Result<CallToolResult, McpError> {
        info!(session_id = %req.session_id, player_name = %req.player_name, "Joining tictactoe session");

        let player_id = format!(
            "{}_{}",
            req.session_id,
            req.player_name.to_lowercase().replace(' ', "_")
        );

        if self.sessions.get_session(&req.session_id).is_none() {
            self.sessions
                .create_session(req.session_id.clone())
                .map_err(|e: String| McpError::internal_error(e, None))?;
        }

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
                McpError::invalid_params(format!("Failed to register: {e}"), None)
            })?;

        info!(player_id = %player_id, mark = ?mark, "Agent registered");

        // Wire peer for notify_tool_list_changed (OnceLock: first call wins).
        self.dynamic.set_peer(peer);

        let game_ctx = TttGameContext {
            session_id: req.session_id.clone(),
            player_id: player_id.clone(),
            sessions: self.sessions.clone(),
            dynamic: self.dynamic.clone(),
        };

        let session = self
            .sessions
            .get_session(&req.session_id)
            .ok_or_else(|| McpError::internal_error("Session not found", None))?;

        let board = session.game.board().display();

        // Record the game briefing in the chat log so the chat pane shows the
        // instructions given to the agent at the start of every game.
        self.sessions.push_dialogue(
            &req.session_id,
            DialogueEntry::server(format!(
                "🎮 {} joined as {:?}. \
                 You are competing in Tic-Tac-Toe against a human opponent. \
                 Win by occupying three squares in a row — horizontally, vertically, or diagonally. \
                 Block your opponent if they are about to win. \
                 Each available tool represents one empty square.",
                req.player_name, mark
            )),
        );

        if session.game.is_over() {
            let result = if let Some(winner) = session.game.winner() {
                let who = if winner == mark {
                    &req.player_name
                } else {
                    "opponent"
                };
                format!("Game already over — {who} wins!\n\n{board}")
            } else {
                format!("Game already over — draw!\n\n{board}")
            };
            return Ok(CallToolResult::success(vec![Content::text(result)]));
        }

        if session.is_players_turn(&player_id) {
            register_move_tools(&self.dynamic, game_ctx, session.game.board());
            self.dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(format!(
                "You are {:?}. It's your turn!\n\n{board}\n\nChoose a square — call one of the `ttt__*` tools.",
                mark
            ))]))
        } else {
            register_await_turn_tool(&game_ctx, "ttt");
            self.dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(format!(
                "You are {:?}. Waiting for opponent's first move.\n\n{board}\n\nCall `ttt__await_turn` once the opponent has moved.",
                mark
            ))]))
        }
    }

    /// Join the shared blackjack table and register bet tools.
    ///
    /// The first caller specifies `num_seats`; subsequent callers join the
    /// same table regardless of that parameter.  Fires
    /// `notify_tool_list_changed` so the agent can place a bet immediately.
    #[instrument(skip(self, peer, req), fields(
        initial_bankroll = req.initial_bankroll,
        session_id = ?req.session_id,
        num_seats = req.num_seats,
    ))]
    #[tool(
        description = "Join the blackjack table. After calling this, use the `bet__place` or `bet__preset_N` tools to place your bet."
    )]
    pub async fn blackjack_deal(
        &self,
        peer: Peer<RoleServer>,
        Parameters(req): Parameters<PlayBlackjackRequest>,
    ) -> Result<CallToolResult, McpError> {
        let bankroll = req.initial_bankroll;
        let num_seats = req.num_seats.max(1) as usize;
        info!(bankroll, num_seats, "Joining blackjack table");

        if bankroll == 0 {
            return Err(McpError::invalid_params(
                "initial_bankroll must be > 0",
                None,
            ));
        }

        // Wire peer so `notify_tool_list_changed` can fire for this connection.
        self.dynamic.set_peer(peer);

        // Initialise (or get) the shared table.
        let table = self.sessions.init_shared_table(num_seats);

        // Join the table — add our seat entry.
        let seat_index = {
            let mut guard = table.lock().await;
            match &mut guard.phase {
                crate::session::SharedTablePhase::Betting { seats, .. } => {
                    let idx = seats.len();
                    let session_id = req
                        .session_id
                        .clone()
                        .unwrap_or_else(|| format!("seat_{idx}"));
                    seats.push(crate::session::SeatEntry {
                        session_id: session_id.clone(),
                        bankroll,
                        bet: None,
                        registry: self.dynamic.clone(),
                    });
                    info!(idx, session_id, "Seat joined table");
                    idx
                }
                _ => {
                    warn!("blackjack_deal called but table is not in Betting phase");
                    return Err(McpError::invalid_params(
                        "Table is already in progress. Wait for the next hand.",
                        None,
                    ));
                }
            }
        };

        // Record seat index on this GameServer instance (idempotent via OnceLock).
        let _ = self.seat_index.set(seat_index);

        // Record session_id on this connection (used by call_tool dialogue logging).
        if let Some(ref sid) = req.session_id {
            let _ = self.session_id.set(sid.clone());
            self.sessions.register_seat_index(sid.clone(), seat_index);
        }

        // Register bet tools for this seat.
        register_bet_tools(
            &self.dynamic,
            BetConstraints {
                min: 1,
                max: bankroll,
                presets: DEFAULT_PRESETS,
            },
            table,
            seat_index,
        );
        self.dynamic.notify_tool_list_changed().await;

        let player_name = req.player_name.as_deref().unwrap_or("Player").to_string();
        let prologue = format!(
            "🃏 Blackjack — Seat {seat}\n\
             Player: {player_name}\n\
             Bankroll: ${bankroll} chips\n\n\
             Rules: Beat the dealer by getting closer to 21 without going over. \
             Face cards are worth 10; Aces are 1 or 11. \
             The dealer hits on soft 16 and stands on 17.\n\n\
             Place your bet to begin: use `bet__place` (custom amount 1–{bankroll}) \
             or a `bet__preset_N` shortcut. \
             View tools (hand, dealer card, shoe, bankroll) become available \
             once cards are dealt.",
            seat = seat_index + 1,
        );

        if let Some(session_id) = &req.session_id {
            self.sessions
                .push_dialogue(session_id, DialogueEntry::server(prologue.clone()));
        }

        Ok(CallToolResult::success(vec![Content::text(prologue)]))
    }
}

impl ServerHandler for GameServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerInfo::new(capabilities).with_instructions(
            "Strictly Games MCP server — type-safe casino games with formal verification.",
        )
    }

    #[instrument(skip(self, _request, _context))]
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let mut tools = self.tool_router.list_all();
        tools.extend(self.dynamic.list_tools());
        // Prefix plugin tool names with the plugin namespace so they appear as
        // `type_spec__describe_type` and `type_spec__explore_type`.
        for t in self.type_spec.list_tools() {
            tools.push(rmcp::model::Tool::new(
                format!("type_spec__{}", t.name),
                t.description.unwrap_or_default(),
                t.input_schema,
            ));
        }
        debug!(count = tools.len(), "Listing tools");
        std::future::ready(Ok(ListToolsResult {
            tools,
            ..Default::default()
        }))
    }

    #[instrument(skip(self, context), fields(tool = %request.name))]
    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            if self.tool_router.has_route(request.name.as_ref()) {
                let ctx = ToolCallContext::new(self, request, context);
                return self.tool_router.call(ctx).await;
            }
            // Route type_spec__ prefixed tools to the TypeSpecPlugin.
            if let Some(bare) = request.name.strip_prefix("type_spec__") {
                let bare_name: std::borrow::Cow<'static, str> = bare.to_string().into();
                let mut inner = request;
                inner.name = bare_name;
                return self
                    .type_spec
                    .call_tool(inner, context)
                    .await
                    .map_err(|e| McpError::internal_error(e.message, None));
            }

            // Dynamic tool — log agent call and server response as dialogue.
            let tool_name = request.name.clone();
            if let Some(session_id) = self.session_id.get() {
                self.sessions
                    .push_dialogue(session_id, DialogueEntry::agent(format!("→ `{tool_name}`")));
            }

            let result = self.dynamic.call_tool(request, context).await?;

            if let Some(session_id) = self.session_id.get() {
                let response_text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_ref()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !response_text.is_empty() {
                    self.sessions
                        .push_dialogue(session_id, DialogueEntry::server(response_text));
                }
            }

            Ok(result)
        }
    }
}
