//! Dynamic tool factory for tictactoe move elicitation.
//!
//! All game state transitions are handled inside tool handlers — `play_game`
//! returns immediately after registering the first batch of tools.  This is
//! essential: AI agents make sequential requests and cannot call a second tool
//! while blocked waiting for the first to return.
//!
//! ## Flow
//! 1. `play_game` → registers `ttt__*` move tools (or `ttt__await_turn`) → returns
//! 2. Agent calls `ttt__center` → move made, `ttt__await_turn` registered → returns
//! 3. Agent calls `ttt__await_turn` (once the opponent has moved) → new `ttt__*` tools registered → returns
//! 4. Repeat until game ends.

use std::sync::Arc;

use crate::session::{DialogueEntry, SessionManager};
use elicitation::{ContextualFactory, DynamicToolDescriptor, DynamicToolRegistry};
use rmcp::model::{CallToolResult, Content, ErrorData};
use serde_json::json;
use strictly_tictactoe::{Position, TicTacToeView};
use tracing::instrument;

// ── Shared context ────────────────────────────────────────────────────────────

/// Context shared by all ttt factories.
#[derive(Clone)]
pub struct TttGameContext {
    pub session_id: String,
    pub player_id: String,
    pub sessions: SessionManager,
    pub dynamic: DynamicToolRegistry,
}

// ── Move factory ──────────────────────────────────────────────────────────────

/// Produces one `ttt__{snake_name}` tool per empty square.
///
/// Calling a tool makes the move, records it in the chat log, registers
/// `ttt__await_turn` (since the opponent always moves next in tictactoe),
/// and returns the updated board state.  If the move ends the game, all
/// ttt tools are cleared instead.
pub struct TttMoveFactory;

pub struct TttMoveContext {
    pub empty_squares: Vec<Position>,
    pub game_ctx: TttGameContext,
}

impl ContextualFactory for TttMoveFactory {
    type Context = TttMoveContext;

    #[instrument(skip(self, ctx), fields(empty = ctx.empty_squares.len()))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &TttMoveContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let mut tools = Vec::new();

        // --- Move tools (one per empty square) ---
        for &pos in &ctx.empty_squares {
            let tool_name = format!("{prefix}__{}", position_snake(pos));
            let game_ctx = ctx.game_ctx.clone();
            let prefix_str = prefix.to_string();

            tools.push(DynamicToolDescriptor {
                name: tool_name,
                description: format!("Play at the {} square.", pos.label()),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let game_ctx = game_ctx.clone();
                    let prefix_str = prefix_str.clone();
                    Box::pin(async move { handle_move(pos, prefix_str, game_ctx).await })
                }),
            });
        }

        // --- Explore tools (live board introspection via TicTacToeView) ---
        // These let the agent query board state before committing to a move.
        // Each call is recorded as an explore (not a play) in ExploreStats.
        let explore_tools: &[(&str, &str, &str)] = &[
            (
                "view_board",
                "board",
                "View the current board layout as a grid.",
            ),
            (
                "view_legal_moves",
                "legal_moves",
                "List all empty positions you can play.",
            ),
            (
                "view_threats",
                "threats",
                "Show immediate win/block opportunities.",
            ),
        ];
        for &(suffix, category, desc) in explore_tools {
            let tool_name = format!("{prefix}__{suffix}");
            let game_ctx = ctx.game_ctx.clone();
            let category = category.to_string();

            tools.push(DynamicToolDescriptor {
                name: tool_name,
                description: desc.to_string(),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let game_ctx = game_ctx.clone();
                    let category = category.clone();
                    Box::pin(async move { handle_explore(category, game_ctx).await })
                }),
            });
        }

        Ok(tools)
    }
}

async fn handle_move(
    pos: Position,
    prefix: String,
    ctx: TttGameContext,
) -> Result<CallToolResult, ErrorData> {
    // Load, mutate, and save session.
    let mut session = ctx
        .sessions
        .get_session(&ctx.session_id)
        .ok_or_else(|| ErrorData::internal_error("Session not found", None))?;

    // Record as a commit (resets per-turn explore counter in ExploreStats).
    ctx.sessions.record_play(&ctx.session_id);

    // Record what the agent saw and chose — developer introspection into the
    // agent's decision process.
    let board_before = session.game.board().display();
    ctx.sessions.push_dialogue(
        &ctx.session_id,
        DialogueEntry::agent(format!(
            "→ `ttt__{}` ({})\n\n{board_before}",
            position_snake(pos),
            pos.label()
        )),
    );

    session
        .make_move(&ctx.player_id, pos)
        .map_err(|e| ErrorData::invalid_params(e, None))?;

    let game_over = session.game.is_over();
    let board = session.game.board().display();
    ctx.sessions
        .update_game_atomic(&ctx.session_id, session.game.clone())
        .map_err(|e| ErrorData::internal_error(e, None))?;

    if game_over {
        // Clear all ttt tools — game is done.
        let _ = ctx
            .dynamic
            .clone()
            .register_contextual(prefix, TttClearFactory, ());
        ctx.dynamic.notify_tool_list_changed().await;

        let result_text = if let Some(winner) = session.game.winner() {
            let who = if winner == session.game.to_move().unwrap_or(winner) {
                "You win"
            } else {
                "Opponent wins"
            };
            format!("🎉 Game over — {who}!\n\n{board}")
        } else {
            format!("🤝 Draw!\n\n{board}")
        };
        ctx.sessions
            .push_dialogue(&ctx.session_id, DialogueEntry::server(result_text.clone()));
        return Ok(CallToolResult::success(vec![Content::text(result_text)]));
    }

    // Opponent plays next — register await_turn and return.
    register_await_turn_tool(&ctx, &prefix);
    ctx.dynamic.notify_tool_list_changed().await;

    let msg = format!(
        "✓ Played {}. Waiting for opponent…\n\n{board}\n\nCall `{prefix}__await_turn` when ready.",
        pos.label()
    );
    ctx.sessions
        .push_dialogue(&ctx.session_id, DialogueEntry::server(msg.clone()));
    Ok(CallToolResult::success(vec![Content::text(msg)]))
}

/// Handles a live board-state exploration query.
///
/// Loads the current board, builds a [`TicTacToeView`] snapshot, and returns
/// the requested category (board / legal_moves / threats).  Records the call
/// in [`ExploreStats`] so the TUI can detect whirlpool behaviour.
async fn handle_explore(
    category: String,
    ctx: TttGameContext,
) -> Result<CallToolResult, ErrorData> {
    // Count this as an explore (not a commit).
    ctx.sessions.record_explore(&ctx.session_id);

    let session = ctx
        .sessions
        .get_session(&ctx.session_id)
        .ok_or_else(|| ErrorData::internal_error("Session not found", None))?;

    let current_player = session
        .get_player(&ctx.player_id)
        .map(|p| p.mark)
        .ok_or_else(|| ErrorData::internal_error("player not found", None))?;

    let view = TicTacToeView::from_board(session.game.board(), current_player);
    let response = view
        .describe_category(&category)
        .unwrap_or_else(|| format!("Unknown category: {category}"));

    // Push to chat so the developer sees what the agent queried.
    ctx.sessions.push_dialogue(
        &ctx.session_id,
        DialogueEntry::agent(format!("🔍 `ttt__view_{category}` → {response}")),
    );

    Ok(CallToolResult::success(vec![Content::text(response)]))
}

// ── Await-turn factory ────────────────────────────────────────────────────────

/// Produces a single `ttt__await_turn` tool.
///
/// The agent calls this after the opponent's move.  The tool checks whether
/// it is now the agent's turn:
/// - Yes → registers move tools, returns board + "Your turn".
/// - No  → returns "Still waiting" (agent should call again shortly).
/// - Game over → clears tools, returns result.
pub struct TttAwaitFactory;

impl ContextualFactory for TttAwaitFactory {
    type Context = TttGameContext;

    #[instrument(skip(self, ctx))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &TttGameContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let tool_name = format!("{prefix}__await_turn");
        let game_ctx = ctx.clone();
        let prefix_str = prefix.to_string();

        Ok(vec![DynamicToolDescriptor {
            name: tool_name,
            description: "Wait for your turn. Call this after the opponent has moved to check if it is now your turn.".to_string(),
            schema: json!({ "type": "object", "properties": {} }),
            handler: Arc::new(move |_args| {
                let game_ctx = game_ctx.clone();
                let prefix_str = prefix_str.clone();
                Box::pin(async move { handle_await_turn(prefix_str, game_ctx).await })
            }),
        }])
    }
}

async fn handle_await_turn(
    prefix: String,
    ctx: TttGameContext,
) -> Result<CallToolResult, ErrorData> {
    // Record the poll in the chat log (suppressed if opponent hasn't moved yet
    // to avoid flooding the pane — only show when state actually changes).
    let session = ctx
        .sessions
        .get_session(&ctx.session_id)
        .ok_or_else(|| ErrorData::internal_error("Session not found", None))?;

    if !session.affirm_continue() {
        let _ = ctx
            .dynamic
            .clone()
            .register_contextual(prefix, TttClearFactory, ());
        ctx.dynamic.notify_tool_list_changed().await;
        return Ok(CallToolResult::success(vec![Content::text(
            "Game cancelled.".to_string(),
        )]));
    }

    if session.game.is_over() {
        let board = session.game.board().display();
        let _ = ctx
            .dynamic
            .clone()
            .register_contextual(prefix, TttClearFactory, ());
        ctx.dynamic.notify_tool_list_changed().await;
        let result = if let Some(winner) = session.game.winner() {
            let who = if session.game.to_move().is_none() {
                // game.to_move() is None when game is over; check history parity
                let agent_mark = session
                    .get_player(&ctx.player_id)
                    .map(|p| p.mark)
                    .ok_or_else(|| ErrorData::internal_error("player not found", None))?;
                if winner == agent_mark {
                    "You win"
                } else {
                    "Opponent wins"
                }
            } else {
                "Unknown outcome"
            };
            format!("🎉 Game over — {who}!\n\n{board}")
        } else {
            format!("🤝 Draw!\n\n{board}")
        };
        ctx.sessions.push_dialogue(
            &ctx.session_id,
            DialogueEntry::agent("→ `ttt__await_turn`".to_string()),
        );
        ctx.sessions
            .push_dialogue(&ctx.session_id, DialogueEntry::server(result.clone()));
        return Ok(CallToolResult::success(vec![Content::text(result)]));
    }

    let board = session.game.board().display();

    if session.is_players_turn(&ctx.player_id) {
        // It's our turn — register move tools.
        let empty = Position::valid_moves(session.game.board());
        let _ = ctx.dynamic.clone().register_contextual(
            prefix.clone(),
            TttMoveFactory,
            TttMoveContext {
                empty_squares: empty,
                game_ctx: ctx.clone(),
            },
        );
        ctx.dynamic.notify_tool_list_changed().await;
        let msg = format!(
            "Your turn!\n\n{board}\n\nChoose a square: call one of the `{prefix}__*` tools."
        );
        ctx.sessions.push_dialogue(
            &ctx.session_id,
            DialogueEntry::agent("→ `ttt__await_turn`".to_string()),
        );
        ctx.sessions
            .push_dialogue(&ctx.session_id, DialogueEntry::server(msg.clone()));
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    } else {
        // Still waiting for opponent — don't flood the chat log with silent polls.
        let msg = format!(
            "Opponent has not moved yet.\n\n{board}\n\nCall `{prefix}__await_turn` again in a moment."
        );
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

// ── Clear factory ─────────────────────────────────────────────────────────────

/// Clears the `ttt` prefix by producing zero tools.
pub struct TttClearFactory;

impl ContextualFactory for TttClearFactory {
    type Context = ();
    fn instantiate(
        &self,
        _prefix: &str,
        _ctx: &(),
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        Ok(vec![])
    }
}

// ── Registration helpers ──────────────────────────────────────────────────────

/// Register move tools for the current board state.
pub fn register_move_tools(
    dynamic: &DynamicToolRegistry,
    ctx: TttGameContext,
    board: &strictly_tictactoe::Board,
) {
    let empty = Position::valid_moves(board);
    let _ = dynamic.clone().register_contextual(
        "ttt",
        TttMoveFactory,
        TttMoveContext {
            empty_squares: empty,
            game_ctx: ctx,
        },
    );
}

/// Register the `ttt__await_turn` tool.
pub fn register_await_turn_tool(ctx: &TttGameContext, prefix: &str) {
    let _ =
        ctx.dynamic
            .clone()
            .register_contextual(prefix.to_string(), TttAwaitFactory, ctx.clone());
}

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Convert a [`Position`] to a snake_case tool suffix.
pub fn position_snake(pos: Position) -> &'static str {
    match pos {
        Position::TopLeft => "top_left",
        Position::TopCenter => "top_center",
        Position::TopRight => "top_right",
        Position::MiddleLeft => "middle_left",
        Position::Center => "center",
        Position::MiddleRight => "middle_right",
        Position::BottomLeft => "bottom_left",
        Position::BottomCenter => "bottom_center",
        Position::BottomRight => "bottom_right",
    }
}
