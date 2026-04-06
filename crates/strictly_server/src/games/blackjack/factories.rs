//! Dynamic tool factories for blackjack elicitation.
//!
//! Each factory drives a single phase transition in the shared blackjack table
//! state machine.  Handlers capture the `SharedTable` and their `seat_index`,
//! and re-register tools on each transition so every player always sees only
//! the valid next moves for their seat.
//!
//! | Factory | Context | Tools | Transition |
//! |---|---|---|---|
//! | [`BetAmountFactory`] | [`BettingContext`] | `bet__place`, `bet__preset_N` | → PlayerTurns (when last bet) |
//! | [`BlackjackActionFactory`] | [`ActionContext`] | `blackjack__hit`, `blackjack__stand`, … | → next seat / DealerTurn / Finished |
//! | [`NextHandFactory`] | [`NextContext`] | `next__deal_again`, `next__cash_out` | → Betting / Idle |
//! | [`ClearFactory`] | `()` | _(none)_ | Clears a prefix |

use std::sync::Arc;

use elicitation::{ContextualFactory, DynamicToolDescriptor, DynamicToolRegistry};
use rmcp::model::{CallToolResult, Content, ErrorData};
use serde_json::json;
use strictly_blackjack::{
    BasicAction, BlackjackPlayerView, MultiRound, PlayerActionContext, SeatBet, Shoe,
};
use tracing::{info, instrument, warn};

use crate::session::{SharedTable, SharedTablePhase};

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
    /// Shared table — all seats share this.
    pub table: SharedTable,
    /// Which seat this connection owns.
    pub seat_index: usize,
    /// This connection's own registry (for local tool updates).
    pub dynamic: DynamicToolRegistry,
}

/// Produces `bet__place` and `bet__preset_N` tools bounded by the player's bankroll.
pub struct BetAmountFactory;

impl ContextualFactory for BetAmountFactory {
    type Context = BettingContext;

    #[instrument(skip(self, ctx), fields(min = ctx.constraints.min, max = ctx.constraints.max, seat = ctx.seat_index))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &BettingContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let min = ctx.constraints.min;
        let max = ctx.constraints.max;
        let table = ctx.table.clone();
        let seat_index = ctx.seat_index;
        let dynamic = ctx.dynamic.clone();

        let mut tools = Vec::new();

        // Fast-path: agent supplies amount directly.
        let place_name = format!("{prefix}__place");
        tools.push({
            let table = table.clone();
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
                    let table = table.clone();
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
                        place_bet_and_transition(amount, seat_index, table, dynamic).await
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
            let table = table.clone();
            let dynamic = dynamic.clone();
            let preset_name = format!("{prefix}__preset_{preset}");
            tools.push(DynamicToolDescriptor {
                name: preset_name,
                description: format!("Place a preset bet of {preset} chips."),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let table = table.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        place_bet_and_transition(preset, seat_index, table, dynamic).await
                    })
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
    /// Shared table.
    pub table: SharedTable,
    /// Which seat this connection owns.
    pub seat_index: usize,
    /// This connection's own registry (for local tool updates).
    pub dynamic: DynamicToolRegistry,
}

/// Produces action tools valid for the current hand state.
pub struct BlackjackActionFactory;

impl ContextualFactory for BlackjackActionFactory {
    type Context = ActionContext;

    #[instrument(skip(self, ctx), fields(
        can_double = ctx.player_ctx.can_double,
        seat = ctx.seat_index,
    ))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &ActionContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let mut tools = Vec::new();
        let table = ctx.table.clone();
        let seat_index = ctx.seat_index;
        let dynamic = ctx.dynamic.clone();
        let bankroll = ctx.bankroll;

        let mut push_action = |name: String, description: &str, action: BasicAction| {
            let desc = description.to_string();
            let table = table.clone();
            let dynamic = dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name,
                description: desc,
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let table = table.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        take_action_and_transition(action, seat_index, table, dynamic).await
                    })
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
                BasicAction::Hit,
            );
        }

        // ── Explore tools ────────────────────────────────────────────────────
        let table_for_explore = ctx.table.clone();
        let make_explore = |category: &'static str, description: &str| {
            let table = table_for_explore.clone();
            DynamicToolDescriptor {
                name: format!("{prefix}__view_{category}"),
                description: description.to_string(),
                schema: json!({ "type": "object", "properties": {} }),
                handler: Arc::new(move |_args| {
                    let table = table.clone();
                    Box::pin(async move {
                        let guard = table.lock().await;
                        let text = match &guard.phase {
                            SharedTablePhase::PlayerTurns { round, .. } => {
                                let view = BlackjackPlayerView::from_multi_round(round, seat_index, bankroll);
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
    /// Shared table.
    pub table: SharedTable,
    /// Which seat this connection owns.
    pub seat_index: usize,
    /// This connection's own registry (for local tool updates).
    pub dynamic: DynamicToolRegistry,
}

/// Produces `next__deal_again` and `next__cash_out` tools.
pub struct NextHandFactory;

impl ContextualFactory for NextHandFactory {
    type Context = NextContext;

    #[instrument(skip(self, ctx), fields(bankroll = ctx.bankroll, seat = ctx.seat_index))]
    fn instantiate(
        &self,
        prefix: &str,
        ctx: &NextContext,
    ) -> Result<Vec<DynamicToolDescriptor>, ErrorData> {
        let mut tools = Vec::new();
        let bankroll = ctx.bankroll;
        let no_params = json!({ "type": "object", "properties": {} });

        if bankroll > 0 {
            let table = ctx.table.clone();
            let seat_index = ctx.seat_index;
            let dynamic = ctx.dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name: format!("{prefix}__deal_again"),
                description: format!(
                    "Deal another hand with your remaining bankroll of {bankroll} chips."
                ),
                schema: no_params.clone(),
                handler: Arc::new(move |_args| {
                    let table = table.clone();
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        deal_again_vote(seat_index, bankroll, table, dynamic).await
                    })
                }),
            });
        }

        {
            let dynamic = ctx.dynamic.clone();
            tools.push(DynamicToolDescriptor {
                name: format!("{prefix}__cash_out"),
                description: format!("End the session with {bankroll} chips."),
                schema: no_params,
                handler: Arc::new(move |_args| {
                    let dynamic = dynamic.clone();
                    Box::pin(async move {
                        clear_prefix(&dynamic, "next");
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
    table: SharedTable,
    seat_index: usize,
) {
    let _ = dynamic.clone().register_contextual(
        "bet",
        BetAmountFactory,
        BettingContext {
            constraints,
            table,
            seat_index,
            dynamic: dynamic.clone(),
        },
    );
}

/// Replace or add action tools in the registry.
pub fn register_action_tools(
    dynamic: &DynamicToolRegistry,
    player_ctx: PlayerActionContext,
    bankroll: u64,
    table: SharedTable,
    seat_index: usize,
) {
    let _ = dynamic.clone().register_contextual(
        "blackjack",
        BlackjackActionFactory,
        ActionContext {
            player_ctx,
            bankroll,
            table,
            seat_index,
            dynamic: dynamic.clone(),
        },
    );
}

/// Replace or add next-hand decision tools in the registry.
pub fn register_next_tools(
    dynamic: &DynamicToolRegistry,
    bankroll: u64,
    table: SharedTable,
    seat_index: usize,
) {
    let _ = dynamic.clone().register_contextual(
        "next",
        NextHandFactory,
        NextContext {
            bankroll,
            table,
            seat_index,
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

/// Place a bet for `seat_index`, advance shared table state, re-register tools.
///
/// If this is the last bet, deals the round and notifies all seats.
#[instrument(skip(table, dynamic), fields(seat_index, amount))]
async fn place_bet_and_transition(
    amount: u64,
    seat_index: usize,
    table: SharedTable,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    // Collect all registries to notify (built outside the lock).
    let registries_to_notify: Vec<DynamicToolRegistry>;
    let seat0_bankroll: u64;
    let seat0_desc: String;
    let is_last_bet: bool;

    // Extract data from the table, building the MultiRound if this is the last bet.
    {
        let mut guard = table.lock().await;
        let SharedTablePhase::Betting { seats, num_seats } = &mut guard.phase else {
            return Err(ErrorData::invalid_params(
                "Not in betting phase",
                None,
            ));
        };

        // Record this seat's bet.
        if seat_index >= seats.len() {
            return Err(ErrorData::invalid_params(
                format!("seat_index {seat_index} out of range (only {} seats joined)", seats.len()),
                None,
            ));
        }
        seats[seat_index].bet = Some(amount);
        info!(seat_index, amount, "Bet recorded");

        is_last_bet = seats.iter().all(|s| s.bet.is_some());

        if !is_last_bet {
            // Not ready yet — just clear bet tools for this seat and wait.
            let pending = seats.iter().filter(|s| s.bet.is_none()).count();
            clear_prefix(&dynamic, "bet");
            dynamic.notify_tool_list_changed().await;
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Bet of {amount} chips placed. Waiting for {pending} more player(s) to bet..."
            ))]));
        }

        // All bets in — deal the round.
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let shoe = Shoe::new(seed, 1);

        let seat_bets: Vec<SeatBet> = seats
            .iter()
            .map(|s| SeatBet {
                name: s.session_id.clone(),
                bankroll: s.bankroll,
                bet: s.bet.expect("all bets set"),
            })
            .collect();

        let seat_registries: Vec<DynamicToolRegistry> =
            seats.iter().map(|s| s.registry.clone()).collect();
        let seat_session_ids: Vec<String> = seats.iter().map(|s| s.session_id.clone()).collect();
        // Post-bet bankrolls (after debit): bankroll - bet amount.
        let seat_bankrolls: Vec<u64> = seats
            .iter()
            .map(|s| s.bankroll.saturating_sub(s.bet.unwrap_or(0)))
            .collect();

        let round = MultiRound::deal_with_shoe(seat_bets, shoe).map_err(|e| {
            ErrorData::internal_error(format!("deal failed: {e}"), None)
        })?;

        // Check for dealer natural — if so, skip to finished immediately.
        if round.dealer_natural() {
            info!("Dealer natural — settling immediately");
            // Capture dealer hand display before consume-via-settle.
            let dealer_display = round.dealer_hand.display();
            let results = round.settle();
            let final_bankrolls: Vec<u64> = results.iter().map(|r| r.final_bankroll).collect();
            let desc = describe_results(&results, &dealer_display);
            registries_to_notify = seat_registries.clone();
            seat0_bankroll = final_bankrolls.first().copied().unwrap_or(0);
            seat0_desc = desc.clone();

            // Register next tools for each seat.
            for (idx, reg) in seat_registries.iter().enumerate() {
                clear_prefix(reg, "bet");
                register_next_tools(reg, final_bankrolls[idx], table.clone(), idx);
            }

            guard.phase = SharedTablePhase::Finished {
                results,
                seat_registries,
                seat_session_ids,
                seat_bankrolls: final_bankrolls,
                ready_count: 0,
                num_seats: *num_seats,
            };
        } else {
            seat0_bankroll = seat_bankrolls.first().copied().unwrap_or(0);
            let hand = &round.seats[0].hand;
            seat0_desc = format!(
                "Cards dealt! Your hand: {} ({})\nDealer shows: {}\n",
                hand.display(),
                hand.value().best(),
                &round.dealer_hand.cards()[0]
            );

            // Register action tools for seat 0; clear bet for all others.
            let player_ctx = PlayerActionContext {
                can_double: false,
                can_split: false,
                can_surrender: false,
            };
            let s0_reg = &seat_registries[0];
            clear_prefix(s0_reg, "bet");
            register_action_tools(s0_reg, player_ctx, seat_bankrolls[0], table.clone(), 0);

            for reg in seat_registries.iter().skip(1) {
                clear_prefix(reg, "bet");
            }

            registries_to_notify = seat_registries.clone();

            guard.phase = SharedTablePhase::PlayerTurns {
                round,
                current_seat: 0,
                seat_registries,
                seat_session_ids,
                seat_bankrolls,
            };
        }
    } // mutex released

    // Notify all seats (no lock held).
    for reg in &registries_to_notify {
        reg.notify_tool_list_changed().await;
    }

    let msg = if is_last_bet {
        format!(
            "Bet of {amount} chips placed.\n{seat0_desc}\nBankroll: ${seat0_bankroll}"
        )
    } else {
        format!("Bet of {amount} chips placed.")
    };
    Ok(CallToolResult::success(vec![Content::text(msg)]))
}

/// Apply a player action for `seat_index`, advance shared table state.
#[instrument(skip(table, dynamic), fields(seat_index, action = ?action))]
async fn take_action_and_transition(
    action: BasicAction,
    seat_index: usize,
    table: SharedTable,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    let registries_to_notify: Vec<DynamicToolRegistry>;
    let result_text: String;

    {
        let mut guard = table.lock().await;
        let SharedTablePhase::PlayerTurns {
            round,
            current_seat,
            seat_registries,
            seat_session_ids: _,
            seat_bankrolls,
        } = &mut guard.phase
        else {
            return Err(ErrorData::invalid_params(
                "Not in player-turn phase",
                None,
            ));
        };

        if *current_seat != seat_index {
            return Err(ErrorData::invalid_params(
                format!("Not your turn — seat {} is acting", current_seat),
                None,
            ));
        }

        // Apply action.
        match action {
            BasicAction::Hit => {
                round.seats[seat_index]
                    .hit(&round.shoe)
                    .map_err(|e| ErrorData::internal_error(format!("hit failed: {e}"), None))?;
            }
            BasicAction::Stand => {
                round.seats[seat_index].stand();
            }
        }

        let seat = &round.seats[seat_index];
        let hand_desc = format!(
            "Your hand: {} ({})\nDealer shows: {}\n",
            seat.hand.display(),
            seat.hand.value().best(),
            &round.dealer_hand.cards()[0]
        );

        if !round.seats[seat_index].is_done() {
            // Seat still playing — re-register action tools.
            let player_ctx = PlayerActionContext {
                can_double: false,
                can_split: false,
                can_surrender: false,
            };
            clear_prefix(&dynamic, "blackjack");
            register_action_tools(&dynamic, player_ctx, seat_bankrolls[seat_index], table.clone(), seat_index);
            registries_to_notify = vec![dynamic.clone()];
            result_text = hand_desc;
        } else {
            // Seat done — advance to next undone seat.
            let num_seats = seat_registries.len();
            let next = (*current_seat + 1..num_seats)
                .find(|&i| !round.seats[i].is_done());

            clear_prefix(&dynamic, "blackjack");

            if let Some(next_seat) = next {
                // Another seat to act.
                *current_seat = next_seat;
                let player_ctx = PlayerActionContext {
                    can_double: false,
                    can_split: false,
                    can_surrender: false,
                };
                let next_reg = &seat_registries[next_seat];
                register_action_tools(
                    next_reg,
                    player_ctx,
                    seat_bankrolls[next_seat],
                    table.clone(),
                    next_seat,
                );
                registries_to_notify = vec![dynamic.clone(), next_reg.clone()];
                result_text = format!(
                    "{hand_desc}Your turn is done. Waiting for other players...",
                );
            } else {
                // All player turns complete — play dealer, then settle.
                // First play dealer (mut borrow of round inside phase).
                if let SharedTablePhase::PlayerTurns { round, .. } = &mut guard.phase {
                    round.play_dealer();
                }

                // Capture dealer display before moving round.
                let dealer_desc = if let SharedTablePhase::PlayerTurns { round, .. } = &guard.phase {
                    format!(
                        "Dealer's hand: {} ({})\n",
                        round.dealer_hand.display(),
                        round.dealer_hand.value().best()
                    )
                } else {
                    String::new()
                };

                // Swap phase out to take owned round for settle.
                let old_phase = std::mem::replace(
                    &mut guard.phase,
                    SharedTablePhase::Betting { seats: vec![], num_seats: 0 },
                );

                let (round, seat_registries, seat_session_ids) = match old_phase {
                    SharedTablePhase::PlayerTurns {
                        round,
                        seat_registries,
                        seat_session_ids,
                        ..
                    } => (round, seat_registries, seat_session_ids),
                    _ => unreachable!(),
                };

                let dealer_display = round.dealer_hand.display();
                let results = round.settle();
                let final_bankrolls: Vec<u64> = results.iter().map(|r| r.final_bankroll).collect();
                let summary = describe_results(&results, &dealer_display);
                let num_seats_val = seat_registries.len();

                for (idx, reg) in seat_registries.iter().enumerate() {
                    register_next_tools(reg, final_bankrolls[idx], table.clone(), idx);
                }

                registries_to_notify = seat_registries.clone();

                guard.phase = SharedTablePhase::Finished {
                    results,
                    seat_registries,
                    seat_session_ids,
                    seat_bankrolls: final_bankrolls,
                    ready_count: 0,
                    num_seats: num_seats_val,
                };

                result_text = format!("{dealer_desc}{summary}");
            }
        }
    } // mutex released

    for reg in &registries_to_notify {
        reg.notify_tool_list_changed().await;
    }

    Ok(CallToolResult::success(vec![Content::text(result_text)]))
}

/// Cast a "deal again" vote. When all seats have voted, starts a new round.
#[instrument(skip(table, dynamic), fields(seat_index, bankroll))]
async fn deal_again_vote(
    seat_index: usize,
    bankroll: u64,
    table: SharedTable,
    dynamic: DynamicToolRegistry,
) -> Result<CallToolResult, ErrorData> {
    let registries_to_notify: Vec<DynamicToolRegistry>;
    let result_text: String;

    {
        let mut guard = table.lock().await;
        let SharedTablePhase::Finished {
            seat_registries,
            seat_session_ids,
            seat_bankrolls,
            ready_count,
            num_seats,
            ..
        } = &mut guard.phase
        else {
            return Err(ErrorData::invalid_params("Not in finished phase", None));
        };

        *ready_count += 1;
        let quorum = *num_seats;
        let current_ready = *ready_count;

        info!(seat_index, current_ready, quorum, "Deal-again vote");

        if current_ready < quorum {
            // Waiting for more votes.
            clear_prefix(&dynamic, "next");
            dynamic.notify_tool_list_changed().await;
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Voted to deal again ({current_ready}/{quorum}). Waiting for other players..."
            ))]));
        }

        // All voted — start new round.
        let final_bankrolls = seat_bankrolls.clone();
        let all_regs = seat_registries.clone();
        let all_session_ids = seat_session_ids.clone();
        let num = quorum;

        // Register bet tools for everyone.
        for (idx, reg) in all_regs.iter().enumerate() {
            let br = final_bankrolls[idx];
            clear_prefix(reg, "next");
            register_bet_tools(
                reg,
                BetConstraints { min: 1, max: br, presets: DEFAULT_PRESETS },
                table.clone(),
                idx,
            );
        }

        registries_to_notify = all_regs.clone();

        let seats = all_regs
            .iter()
            .enumerate()
            .map(|(idx, reg)| crate::session::SeatEntry {
                session_id: all_session_ids[idx].clone(),
                bankroll: final_bankrolls[idx],
                bet: None,
                registry: reg.clone(),
            })
            .collect();

        guard.phase = SharedTablePhase::Betting {
            seats,
            num_seats: num,
        };

        result_text = format!(
            "💰 New hand! Bankroll: ${bankroll}. Place your bet."
        );
    }

    for reg in &registries_to_notify {
        reg.notify_tool_list_changed().await;
    }

    Ok(CallToolResult::success(vec![Content::text(result_text)]))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Summarise all seat results for the finished-hand display.
fn describe_results(results: &[strictly_blackjack::SeatResult], dealer_display: &str) -> String {
    let mut s = format!("Dealer: {dealer_display}\n\n");
    for r in results {
        let payout_line = match r.outcome {
            strictly_blackjack::Outcome::Win | strictly_blackjack::Outcome::Blackjack => {
                format!("Won: ${}\n", r.bet)
            }
            strictly_blackjack::Outcome::Loss => format!("Lost: ${}\n", r.bet),
            strictly_blackjack::Outcome::Push => "Push\n".to_string(),
            strictly_blackjack::Outcome::Surrender => "Surrendered (half bet returned)\n".to_string(),
        };
        s.push_str(&format!(
            "{}: {} — {}\n{}\n💰 Bankroll: ${}\n\n",
            r.name,
            r.hand.display(),
            r.outcome,
            payout_line,
            r.final_bankroll
        ));
    }
    s
}
