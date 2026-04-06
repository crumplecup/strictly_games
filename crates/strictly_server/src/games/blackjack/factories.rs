//! Dynamic tool factories for blackjack elicitation.
//!
//! Each factory drives a single phase transition in the blackjack typestate
//! machine.  Handlers capture shared game state and re-register tools on
//! each transition so the agent always sees only the valid next moves.
//!
//! | Factory | Context | Tools | Transition |
//! |---|---|---|---|
//! | [`BetAmountFactory`] | [`BettingContext`] | `bet__place`, `bet__preset_N` | → PlayerTurn |
//! | [`BlackjackActionFactory`] | [`ActionContext`] | `blackjack__hit`, `blackjack__stand`, … | → DealerTurn / Finished |
//! | [`NextHandFactory`] | [`NextContext`] | `next__deal_again`, `next__cash_out` | → Betting / Idle |
//! | [`ClearFactory`] | `()` | _(none)_ | Clears a prefix |

use std::sync::Arc;

use elicitation::{ContextualFactory, DynamicToolDescriptor, DynamicToolRegistry};
use rmcp::model::{CallToolResult, Content, ErrorData};
use serde_json::json;
use strictly_blackjack::{
    BasicAction, BlackjackPlayerView, GameResult, GameSetup, PlayerAction, PlayerActionContext,
};
use tracing::instrument;

use super::session::{BlackjackPhase, BlackjackSession, describe_finished, describe_player_turn};

// ── Bet amount ────────────────────────────────────────────────────────────────

/// Standard preset bet sizes offered to the agent.
pub const DEFAULT_PRESETS: &[u64] = &[50, 100, 200, 500];

/// Runtime constraints for bet amount elicitation.
pub struct BetConstraints {
    /// Minimum allowed bet (typically 1).
    pub min: u64,
    /// Maximum allowed bet — the player's current bankroll.
    pub max: u64,
    /// Preset sizes to offer; any preset exceeding `max` is silently omitted.
    pub presets: &'static [u64],
}

/// Context passed to [`BetAmountFactory`] at betting phase start.
pub struct BettingContext {
    /// Bet bounds derived from current bankroll.
    pub constraints: BetConstraints,
    /// Shared game phase — will be advanced inside the handler.
    pub phase: BlackjackSession,
    /// Registry for re-registering tools on phase transition.
    pub dynamic: DynamicToolRegistry,
}

/// Produces `bet__place` and `bet__preset_N` tools bounded by the player's bankroll.
pub struct BetAmountFactory;

impl ContextualFactory for BetAmountFactory {
    type Context = BettingContext;

    #[instrument(skip(self, ctx), fields(min = ctx.constraints.min, max = ctx.constraints.max))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &BettingContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let min = ctx.constraints.min;
        let max = ctx.constraints.max;
        let phase = ctx.phase.clone();
        let dynamic = ctx.dynamic.clone();

        let mut tools = Vec::new();

        // Fast-path: agent supplies amount directly.
        let place_name = format!("{prefix}__place");
        tools.push({
            let phase = phase.clone();
            let dynamic = dynamic.clone();
            DynamicToolDescriptor {
                name: place_name,
                description: format!("Place a bet between {min} and {max} chips."),
                schema: json!({
                    "type": "object",
                    "properties": {
                        "amount": {
                            "type": "integer",
                            "minimum": min,
                            "maximum": max,
                            "description": "Number of chips to bet."
                        }
                    },
                    "required": ["amount"]
                }),
                handler: Arc::new(move |args| {
                    let phase = phase.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        let amount =
                            args.get("amount").and_then(|v| v.as_u64()).ok_or_else(|| {
                                ErrorData::invalid_params("`amount` must be a u64", None)
                            })?;
                        if amount < min || amount > max {
                            return Err(ErrorData::invalid_params(
                                format!("amount {amount} out of range [{min}, {max}]"),
                                None,
                            ));
                        }
                        place_bet_and_transition(amount, phase, dynamic).await
                    })
                }),
            }
        });

        // Preset tools — only those within bankroll.
        for &preset in ctx
            .constraints
            .presets
            .iter()
            .filter(|&&p| p >= min && p <= max)
        {
            let phase = phase.clone();
            let dynamic = dynamic.clone();
            let preset_name = format!("{prefix}__preset_{preset}");
            tools.push(DynamicToolDescriptor {
                name: preset_name,
                description: format!("Place a preset bet of {preset} chips."),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let phase = phase.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move { place_bet_and_transition(preset, phase, dynamic).await })
                }),
            });
        }

        Ok(tools)
    }
}

// ── Player actions ────────────────────────────────────────────────────────────

/// Context passed to [`BlackjackActionFactory`] at player-turn start.
pub struct ActionContext {
    /// Which actions are currently valid.
    pub player_ctx: PlayerActionContext,
    /// Player's bankroll (post-bet balance) — used by explore tools.
    pub bankroll: u64,
    /// Shared game phase.
    pub phase: BlackjackSession,
    /// Registry for re-registering tools on phase transition.
    pub dynamic: DynamicToolRegistry,
}

/// Produces action tools valid for the current hand state.
pub struct BlackjackActionFactory;

impl ContextualFactory for BlackjackActionFactory {
    type Context = ActionContext;

    #[instrument(skip(self, ctx), fields(
        can_double = ctx.player_ctx.can_double,
        can_split = ctx.player_ctx.can_split,
        can_surrender = ctx.player_ctx.can_surrender,
    ))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &ActionContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let mut tools = Vec::new();
        let phase = ctx.phase.clone();
        let dynamic = ctx.dynamic.clone();

        let mut push_action = |name: String, description: &str, action: BasicAction| {
            let desc = description.to_string();
            let phase = phase.clone();
            let dynamic = dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name,
                description: desc,
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let phase = phase.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(
                        async move { take_action_and_transition(action, phase, dynamic).await },
                    )
                }),
            });
        };

        push_action(
            format!("{prefix}__hit"),
            "Take another card.",
            BasicAction::Hit,
        );
        push_action(
            format!("{prefix}__stand"),
            "End your turn and keep your current hand.",
            BasicAction::Stand,
        );

        if ctx.player_ctx.can_double {
            push_action(
                format!("{prefix}__double"),
                "Double your bet and take exactly one more card.",
                BasicAction::Hit, // TODO: implement Double in blackjack crate
            );
        }

        // ── Explore tools (consume-and-remove on agent side) ────────────────
        let bankroll = ctx.bankroll;
        let phase_for_explore = ctx.phase.clone();
        let make_explore = |category: &'static str, description: &str| {
            let phase = phase_for_explore.clone();
            DynamicToolDescriptor {
                name: format!("{prefix}__view_{category}"),
                description: description.to_string(),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let phase = phase.clone();
                    Box::pin(async move {
                        let guard = phase.lock().await;
                        let text = match &*guard {
                            BlackjackPhase::PlayerTurn(game) => {
                                let view = BlackjackPlayerView::from_game_state(game, 0, bankroll);
                                view.describe_category(category)
                                    .unwrap_or_else(|| format!("No data for '{category}'"))
                            }
                            _ => format!("Not in player turn — '{category}' unavailable"),
                        };
                        Ok(CallToolResult::success(vec![Content::text(text)]))
                    })
                }),
            }
        };

        tools.push(make_explore(
            "your_hand",
            "View your current hand and its total value.",
        ));
        tools.push(make_explore(
            "dealer_showing",
            "View the dealer's face-up card.",
        ));
        tools.push(make_explore(
            "shoe_status",
            "View how many cards remain in the shoe.",
        ));
        tools.push(make_explore("bankroll", "View your current bankroll."));

        Ok(tools)
    }
}

// ── Next-hand decision ────────────────────────────────────────────────────────

/// Context passed to [`NextHandFactory`] after a hand completes.
pub struct NextContext {
    /// Player's bankroll after the finished hand.
    pub bankroll: u64,
    /// Shared game phase.
    pub phase: BlackjackSession,
    /// Registry for re-registering tools on transition.
    pub dynamic: DynamicToolRegistry,
}

/// Produces `next__deal_again` and `next__cash_out` tools.
pub struct NextHandFactory;

impl ContextualFactory for NextHandFactory {
    type Context = NextContext;

    #[instrument(skip(self, ctx), fields(bankroll = ctx.bankroll))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &NextContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let mut tools = Vec::new();
        let bankroll = ctx.bankroll;
        let no_params = json!({ "type": "object", "properties": {} });

        if bankroll > 0 {
            let phase = ctx.phase.clone();
            let dynamic = ctx.dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name: format!("{prefix}__deal_again"),
                description: format!(
                    "Deal another hand with your remaining bankroll of {bankroll} chips."
                ),
                schema: no_params.clone(),
                handler: Arc::new(move |_args| {
                    let phase = phase.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move { deal_new_hand(bankroll, phase, dynamic).await })
                }),
            });
        }

        {
            let phase = ctx.phase.clone();
            let dynamic = ctx.dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name: format!("{prefix}__cash_out"),
                description: format!("End the session with {bankroll} chips."),
                schema: no_params,
                handler: Arc::new(move |_args| {
                    let phase = phase.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        clear_prefix(&dynamic, "next");
                        {
                            let mut guard = phase.lock().await;
                            *guard = BlackjackPhase::Idle;
                        }
                        dynamic.notify_tool_list_changed().await;
                        Ok(CallToolResult::success(vec![Content::text(format!(
                            "👋 Thanks for playing! Final bankroll: ${bankroll}"
                        ))]))
                    })
                }),
            });
        }

        Ok(tools)
    }
}

// ── ClearFactory ─────────────────────────────────────────────────────────────

/// Replaces a prefix entry with an empty tool list, effectively removing it.
pub struct ClearFactory;

impl ContextualFactory for ClearFactory {
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

/// Replace or add bet tools in the registry.
pub fn register_bet_tools(
    dynamic: &DynamicToolRegistry,
    constraints: BetConstraints,
    phase: BlackjackSession,
) {
    let _ = dynamic.clone().register_contextual(
        "bet",
        BetAmountFactory,
        BettingContext {
            constraints,
            phase,
            dynamic: dynamic.clone(),
        },
    );
}

/// Replace or add action tools in the registry.
pub fn register_action_tools(
    dynamic: &DynamicToolRegistry,
    player_ctx: PlayerActionContext,
    bankroll: u64,
    phase: BlackjackSession,
) {
    let _ = dynamic.clone().register_contextual(
        "blackjack",
        BlackjackActionFactory,
        ActionContext {
            player_ctx,
            bankroll,
            phase,
            dynamic: dynamic.clone(),
        },
    );
}

/// Replace or add next-hand decision tools in the registry.
pub fn register_next_tools(dynamic: &DynamicToolRegistry, bankroll: u64, phase: BlackjackSession) {
    let _ = dynamic.clone().register_contextual(
        "next",
        NextHandFactory,
        NextContext {
            bankroll,
            phase,
            dynamic: dynamic.clone(),
        },
    );
}

/// Clear all tools registered under `prefix`.
pub fn clear_prefix(dynamic: &DynamicToolRegistry, prefix: &str) {
    let _ = dynamic
        .clone()
        .register_contextual(prefix, ClearFactory, ());
}

// ── Shared transition logic ───────────────────────────────────────────────────

/// Place a bet, advance game state, re-register tools.
async fn place_bet_and_transition(
    amount: u64,
    phase: BlackjackSession,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    // Extract the game, replacing with Idle so the mutex isn't held during transitions.
    let game = {
        let mut guard = phase.lock().await;
        match std::mem::replace(&mut *guard, BlackjackPhase::Idle) {
            BlackjackPhase::Betting(g) => *g,
            other => {
                *guard = other;
                return Err(ErrorData::invalid_params(
                    "Not in betting phase — call blackjack__deal first",
                    None,
                ));
            }
        }
    };

    match game.place_bet(amount) {
        Ok(GameResult::PlayerTurn(player_game)) => {
            let desc = describe_player_turn(&player_game);
            let player_ctx = PlayerActionContext {
                can_double: false,
                can_split: false,
                can_surrender: false,
            };
            let bankroll = player_game.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::PlayerTurn(Box::new(player_game));
            }
            clear_prefix(&dynamic, "bet");
            register_action_tools(&dynamic, player_ctx, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Bet {amount} chips placed.\n{desc}"
            ))]))
        }
        Ok(GameResult::Finished(finished, _)) => {
            let desc = describe_finished(&finished);
            let bankroll = finished.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::Finished;
            }
            clear_prefix(&dynamic, "bet");
            register_next_tools(&dynamic, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(desc)]))
        }
        Ok(GameResult::DealerTurn(dealer_game)) => {
            let (finished, _) = dealer_game.play_dealer_turn();
            let desc = describe_finished(&finished);
            let bankroll = finished.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::Finished;
            }
            clear_prefix(&dynamic, "bet");
            register_next_tools(&dynamic, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(desc)]))
        }
        Err(e) => Err(ErrorData::invalid_params(
            format!("place_bet failed: {e}"),
            None,
        )),
    }
}

/// Apply a player action, advance game state, re-register tools.
async fn take_action_and_transition(
    action: BasicAction,
    phase: BlackjackSession,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    let game = {
        let mut guard = phase.lock().await;
        match std::mem::replace(&mut *guard, BlackjackPhase::Idle) {
            BlackjackPhase::PlayerTurn(g) => *g,
            other => {
                *guard = other;
                return Err(ErrorData::invalid_params(
                    "Not in player turn — call blackjack__deal and bet first",
                    None,
                ));
            }
        }
    };

    let hand_idx = game.current_hand_index();
    let player_action = PlayerAction::new(action, hand_idx);

    match game.take_action(player_action) {
        Ok(GameResult::PlayerTurn(next_game)) => {
            let desc = describe_player_turn(&next_game);
            let player_ctx = PlayerActionContext {
                can_double: false,
                can_split: false,
                can_surrender: false,
            };
            let bankroll = next_game.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::PlayerTurn(Box::new(next_game));
            }
            // Re-register action tools (hand may have changed).
            clear_prefix(&dynamic, "blackjack");
            register_action_tools(&dynamic, player_ctx, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(desc)]))
        }
        Ok(GameResult::DealerTurn(dealer_game)) => {
            let (finished, _) = dealer_game.play_dealer_turn();
            let desc = describe_finished(&finished);
            let bankroll = finished.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::Finished;
            }
            clear_prefix(&dynamic, "blackjack");
            register_next_tools(&dynamic, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(desc)]))
        }
        Ok(GameResult::Finished(finished, _)) => {
            let desc = describe_finished(&finished);
            let bankroll = finished.bankroll();
            {
                let mut guard = phase.lock().await;
                *guard = BlackjackPhase::Finished;
            }
            clear_prefix(&dynamic, "blackjack");
            register_next_tools(&dynamic, bankroll, phase);
            dynamic.notify_tool_list_changed().await;
            Ok(CallToolResult::success(vec![Content::text(desc)]))
        }
        Err(e) => Err(ErrorData::internal_error(
            format!("action failed: {e}"),
            None,
        )),
    }
}

/// Start a new hand from the next-hand phase.
async fn deal_new_hand(
    bankroll: u64,
    phase: BlackjackSession,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    // Verify phase
    {
        let guard = phase.lock().await;
        if !matches!(*guard, BlackjackPhase::Finished) {
            return Err(ErrorData::invalid_params("Not in finished phase", None));
        }
    }

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let game = GameSetup::new(seed).start_betting(bankroll);

    {
        let mut guard = phase.lock().await;
        *guard = BlackjackPhase::Betting(Box::new(game));
    }

    clear_prefix(&dynamic, "next");
    register_bet_tools(
        &dynamic,
        BetConstraints {
            min: 1,
            max: bankroll,
            presets: DEFAULT_PRESETS,
        },
        phase,
    );
    dynamic.notify_tool_list_changed().await;

    Ok(CallToolResult::success(vec![Content::text(format!(
        "💰 New hand started. Bankroll: ${bankroll}. Place your bet."
    ))]))
}
