//! Blackjack TUI game loop.
//!
//! Drives a local blackjack game session entirely within the ratatui terminal.
//! The [`TuiCommunicator`] is used for all player decisions so that the same
//! elicitation interface works for both human players (in the TUI) and future
//! AI agent passengers riding along through the same interface.
//!
//! Game logic is wired through the proof-carrying workflow tools:
//! `execute_place_bet` → `execute_play_action` (loop) → `execute_dealer_turn`.
//! The compiler enforces the correct call order via `Established<P>` contracts.

use crate::tui::contracts::{MultiRoundActive, NoOverflow};
use crate::tui::observable_communicator::ObservableCommunicator;
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    GameEvent, PhaseContext, TypestateGraphWidget, blackjack_active, blackjack_edges,
    blackjack_nodes,
};
use crate::tui::{ChatMessage, LlmElicitCommunicator, Participant, chat_channel};
use crate::{PlayerKind, PlayerSlot};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use elicitation::Elicitation as _;
use elicitation::contracts::{And, Established, Prop, both};
use ratatui::{Terminal, backend::Backend};
use std::time::SystemTime;
use strictly_blackjack::{
    BasicAction, BlackjackAction, BlackjackPlayerView, Hand, MultiRound, Outcome, SeatBet,
};
use tokio::sync::watch;
use tokio::time::{Duration, sleep};
use tracing::{info, instrument, warn};

/// Height of the dedicated prompt pane at the bottom of the game layout.
///
/// Sized for the largest prompt variant: the `BasicAction` enum prompt is
/// 7 lines + 1 input line + 2 border lines = 10 rows.
pub const PROMPT_PANE_HEIGHT: u16 = 10;

/// Outcome of a complete blackjack session, from the player's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlackjackSessionOutcome {
    /// Player won (bankroll after payout).
    Win(u64),
    /// Player lost (bankroll after deduction).
    Loss(u64),
    /// Push — bet returned (bankroll unchanged).
    Push(u64),
    /// Player abandoned the session (pressed `q`).
    Abandoned,
}

impl BlackjackSessionOutcome {}

// ─────────────────────────────────────────────────────────────
//  Shared helpers for multi-player session
// ─────────────────────────────────────────────────────────────

/// Type-erased seat communicator — dispatches elicitation to either the human
/// TUI communicator or an AI agent LLM communicator.
enum SeatComm {
    Human {
        comm: ObservableCommunicator<TuiCommunicator>,
        prompt_rx: watch::Receiver<Option<String>>,
    },
    Agent {
        comm: ObservableCommunicator<LlmElicitCommunicator>,
        prompt_rx: watch::Receiver<Option<String>>,
    },
}

/// Style override for blackjack bet prompts.
#[derive(Debug, Clone)]
struct BlackjackBetStyle {
    max_bet: u64,
}

impl BlackjackBetStyle {
    fn new(max_bet: u64) -> Self {
        Self { max_bet }
    }
}

impl Default for BlackjackBetStyle {
    fn default() -> Self {
        Self { max_bet: 100 }
    }
}

impl elicitation::style::ElicitationStyle for BlackjackBetStyle {
    fn prompt_for_field(
        &self,
        _field_name: &str,
        _field_type: &str,
        _context: &elicitation::style::PromptContext,
    ) -> String {
        format!(
            "Place your bet (1-{}). Respond with a single number only:",
            self.max_bet
        )
    }
}

/// Elicits a valid bet (1..=max_bet) from any communicator.
#[instrument(skip(comm))]
async fn elicit_bet_from<C: elicitation::ElicitCommunicator>(
    comm: &C,
    max_bet: u64,
) -> Option<u64> {
    let styled = comm.with_style::<u64, BlackjackBetStyle>(BlackjackBetStyle::new(max_bet));
    loop {
        match u64::elicit(&styled).await {
            Ok(v) if v > 0 && v <= max_bet => return Some(v),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// Elicits a `BasicAction` from any communicator.
#[instrument(skip(comm))]
async fn elicit_action_from<C: elicitation::ElicitCommunicator>(comm: &C) -> Option<BasicAction> {
    BasicAction::elicit(comm).await.ok()
}

/// Elicits a commit action from an agent, allowing exploration of game state.
#[instrument(skip(comm, round, event_log))]
async fn elicit_agent_action<C: elicitation::ElicitCommunicator + Clone>(
    comm: &C,
    round: &MultiRound,
    seat_idx: usize,
    bankroll: u64,
    event_log: &mut Vec<GameEvent>,
) -> Option<BasicAction> {
    use crate::tui::contextual_communicator::{ContextualCommunicator, knowledge_cache};
    use elicitation::ElicitCommunicator as _;

    let knowledge = knowledge_cache();
    let ctx_comm = ContextualCommunicator::new(comm.clone(), knowledge.clone());

    loop {
        let action = BlackjackAction::elicit(&ctx_comm).await.ok()?;

        if action.is_commit() {
            return action.to_basic_action();
        }

        let category = action.explore_category().unwrap_or("unknown");
        let view = BlackjackPlayerView::from_multi_round(round, seat_idx, bankroll);
        let description = view
            .describe_category(category)
            .unwrap_or_else(|| "No information available".to_string());

        let narration = match action {
            BlackjackAction::ViewHand => "checks their hand",
            BlackjackAction::ViewDealerCard => "checks dealer's up card",
            BlackjackAction::ViewOtherPlayers => "looks at other players",
            BlackjackAction::ViewShoeStatus => "checks the shoe",
            BlackjackAction::ViewBankroll => "checks bankroll",
            _ => "explores",
        };
        event_log.push(GameEvent::story(format!(
            "  🔍 {} {}",
            round.seats[seat_idx].name, narration
        )));

        knowledge
            .lock()
            .unwrap()
            .push(format!("[{category}] {description}"));

        let _ = ctx_comm
            .send_prompt(&format!("[Game State — {category}] {description}"))
            .await;
    }
}

///
/// All players compete independently against the house from a single shared shoe.
/// Chat messages from all seats are collected and displayed in the right panel.
///
/// # Round flow
///
/// 1. Elicit a bet from each active seat (bankroll > 0).
/// 2. Deal via [`MultiRound::deal`].
/// 3. If dealer has a natural: settle immediately.
/// 4. Otherwise: elicit Hit/Stand per seat until all done.
/// 5. Dealer plays deterministically.
/// 6. Settle all seats; update bankrolls.
/// 7. Repeat or exit on bankroll exhaustion / player quit.
#[instrument(skip(terminal, players))]
pub async fn run_multi_blackjack_session<B: Backend>(
    terminal: &mut Terminal<B>,
    players: Vec<PlayerSlot>,
    show_typestate_graph: bool,
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    info!("Starting multi-player blackjack session");

    let (chat_tx, mut chat_rx) = chat_channel();
    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();

    // Build per-seat communicators and state.
    let mut seat_comms: Vec<SeatComm> = Vec::new();
    let mut seat_names: Vec<String> = Vec::new();
    let mut bankrolls: Vec<u64> = Vec::new();

    for slot in players {
        let (prompt_tx, prompt_rx) = watch::channel(None::<String>);
        match slot.kind {
            PlayerKind::Human => {
                let comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx)
                    .with_chat(chat_tx.clone(), Participant::Human);
                seat_comms.push(SeatComm::Human { comm, prompt_rx });
                seat_names.push(slot.name);
                bankrolls.push(slot.bankroll);
            }
            PlayerKind::Agent(ref config) => match LlmElicitCommunicator::new(config) {
                Ok(base) => {
                    let participant = Participant::Agent(slot.name.clone());
                    let comm = ObservableCommunicator::new(base, prompt_tx)
                        .with_chat(chat_tx.clone(), participant);
                    seat_comms.push(SeatComm::Agent { comm, prompt_rx });
                    seat_names.push(slot.name);
                    bankrolls.push(slot.bankroll);
                }
                Err(e) => {
                    warn!(agent = %slot.name, error = %e, "Failed to create agent communicator");
                }
            },
        }
    }

    if seat_comms.is_empty() {
        warn!("No valid seats — aborting multi-player session");
        return Ok(());
    }

    let num_seats = seat_comms.len();
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut chat_messages: Vec<ChatMessage> = Vec::new();

    'session: loop {
        // ── Collect bets ─────────────────────────────────────────────────
        let mut seat_bets: Vec<SeatBet> = Vec::new();
        // Track which `bankrolls` indices are participating this round so we
        // can map `results[j]` back to the correct `bankrolls[active_indices[j]]`.
        let mut active_indices: Vec<usize> = Vec::new();

        // Render the betting phase so the player sees the table before prompts.
        let empty_hand = strictly_blackjack::Hand::default();
        let _ = render_multi(
            terminal,
            MultiRenderCtx {
                seat_names: &seat_names,
                bankrolls: &bankrolls,
                dealer_hand: &empty_hand,
                seats: &[],
                active_seat: usize::MAX,
                chat_messages: &chat_messages,
                bj_nodes: &bj_nodes,
                bj_edges: &bj_edges,
                show_typestate_graph,
                event_log: &event_log,
                prompt: Some("Place your bets!"),
            },
            Established::<MultiRoundActive>::assert(),
        )?;

        for i in 0..num_seats {
            let bankroll = bankrolls[i];
            if bankroll == 0 {
                continue;
            }

            let bet_opt = match &seat_comms[i] {
                SeatComm::Human { comm, .. } => elicit_bet_from(comm, bankroll).await,
                SeatComm::Agent { comm, .. } => elicit_bet_from(comm, bankroll).await,
            };
            let Some(bet) = bet_opt else {
                break 'session;
            };

            event_log.push(GameEvent::story(format!(
                "💰  {} bets {bet} ({bankroll} chips)",
                seat_names[i]
            )));

            active_indices.push(i);
            seat_bets.push(SeatBet {
                name: seat_names[i].clone(),
                bankroll,
                bet,
            });
        }

        if seat_bets.is_empty() {
            break 'session;
        }

        // ── Deal ─────────────────────────────────────────────────────────
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let mut round = MultiRound::deal(seat_bets, seed).map_err(anyhow::Error::msg)?;

        // Narrate the deal: each seat's opening hand and the dealer's up card.
        let dealer_up = round
            .dealer_hand
            .cards()
            .first()
            .map(|c| c.to_string())
            .unwrap_or_default();
        event_log.push(GameEvent::story(format!(
            "🂠  Cards dealt — dealer shows {dealer_up}"
        )));
        for seat in &round.seats {
            let cards: Vec<String> = seat.hand.cards().iter().map(|c| c.to_string()).collect();
            let total = seat.hand.value().best();
            if seat.natural {
                event_log.push(GameEvent::story(format!(
                    "  ⭐ {} dealt {} — Blackjack!",
                    seat.name,
                    cards.join(" "),
                )));
            } else {
                event_log.push(GameEvent::story(format!(
                    "  {} dealt {} ({})",
                    seat.name,
                    cards.join(" "),
                    total,
                )));
            }
        }
        event_log.push(GameEvent::phase_change("Betting", "PlayerTurn"));
        event_log.push(GameEvent::proof("BetPlaced × all seats"));

        // ── Player turns (skip if dealer natural) ────────────────────────
        if !round.dealer_natural() {
            for (round_idx, &bankroll_idx) in active_indices.iter().enumerate() {
                if round.seats[round_idx].is_done() {
                    // Natural — no action needed.
                    continue;
                }

                event_log.push(GameEvent::story(format!(
                    "🎯  {}'s turn",
                    seat_names[bankroll_idx]
                )));

                loop {
                    // Drain any new chat messages before rendering.
                    while let Ok(msg) = chat_rx.try_recv() {
                        chat_messages.push(msg);
                    }

                    let prompt_snapshot = match &seat_comms[bankroll_idx] {
                        SeatComm::Human { prompt_rx, .. } => prompt_rx.borrow().clone(),
                        SeatComm::Agent { prompt_rx, .. } => prompt_rx.borrow().clone(),
                    };

                    let _ = render_multi(
                        terminal,
                        MultiRenderCtx {
                            seat_names: &seat_names,
                            bankrolls: &bankrolls,
                            dealer_hand: &round.dealer_hand,
                            seats: &round.seats,
                            active_seat: round_idx,
                            chat_messages: &chat_messages,
                            bj_nodes: &bj_nodes,
                            bj_edges: &bj_edges,
                            show_typestate_graph,
                            event_log: &event_log,
                            prompt: prompt_snapshot.as_deref(),
                        },
                        Established::<MultiRoundActive>::assert(),
                    )?;

                    if round.seats[round_idx].is_done() {
                        break;
                    }

                    let action_opt = match &seat_comms[bankroll_idx] {
                        SeatComm::Human { comm, .. } => elicit_action_from(comm).await,
                        SeatComm::Agent { comm, .. } => {
                            elicit_agent_action(
                                comm,
                                &round,
                                round_idx,
                                bankrolls[bankroll_idx],
                                &mut event_log,
                            )
                            .await
                        }
                    };
                    let Some(action) = action_opt else {
                        break 'session;
                    };

                    match action {
                        BasicAction::Hit => {
                            let shoe = &round.shoe;
                            round.seats[round_idx]
                                .hit(shoe)
                                .map_err(anyhow::Error::msg)?;
                            let seat = &round.seats[round_idx];
                            let new_card = seat
                                .hand
                                .cards()
                                .last()
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let total = seat.hand.value().best();
                            if seat.bust {
                                event_log.push(GameEvent::story(format!(
                                    "  🃏 {} hits → receives {new_card}, total {total} — bust!",
                                    seat.name,
                                )));
                            } else {
                                event_log.push(GameEvent::story(format!(
                                    "  🃏 {} hits → receives {new_card}, total {total}",
                                    seat.name,
                                )));
                            }
                        }
                        BasicAction::Stand => {
                            round.seats[round_idx].stand();
                            let total = round.seats[round_idx].hand.value().best();
                            event_log.push(GameEvent::story(format!(
                                "  🖐 {} stands at {total}",
                                round.seats[round_idx].name,
                            )));
                        }
                    }
                }
            }
        } else {
            let dealer_cards: Vec<String> = round
                .dealer_hand
                .cards()
                .iter()
                .map(|c| c.to_string())
                .collect();
            event_log.push(GameEvent::story(format!(
                "⚡  Dealer reveals {} — Natural Blackjack!",
                dealer_cards.join(" "),
            )));
        }

        // ── Dealer turn ──────────────────────────────────────────────────
        event_log.push(GameEvent::phase_change("PlayerTurn", "DealerTurn"));
        let dealer_cards_before = round.dealer_hand.cards().len();
        let dealer_cards_str: Vec<String> = round
            .dealer_hand
            .cards()
            .iter()
            .map(|c| c.to_string())
            .collect();
        event_log.push(GameEvent::story(format!(
            "🎰  Dealer reveals hole card: {}",
            dealer_cards_str.join(" "),
        )));

        round.play_dealer();

        // Narrate each card the dealer drew.
        for card in round.dealer_hand.cards().iter().skip(dealer_cards_before) {
            let total = round.dealer_hand.value().best();
            event_log.push(GameEvent::story(format!(
                "  🃏 Dealer draws {card}, total {total}",
            )));
        }
        let dealer_final = round.dealer_hand.value().best();
        if round.dealer_hand.is_bust() {
            event_log.push(GameEvent::story(format!(
                "  Dealer busts at {dealer_final}!",
            )));
        } else {
            event_log.push(GameEvent::story(format!(
                "  Dealer stands at {dealer_final}",
            )));
        }
        event_log.push(GameEvent::phase_change("DealerTurn", "Finished"));

        // ── Settle ───────────────────────────────────────────────────────
        // Snapshot hand state before round is consumed by settle().
        let dealer_hand_snap = round.dealer_hand;
        let seats_snap = round.seats.clone();
        let results = round.settle();
        event_log.push(GameEvent::proof("PayoutSettled × all seats"));

        for result in &results {
            let outcome_str = match result.outcome {
                Outcome::Win => "🎉 Win",
                Outcome::Loss => "💸 Loss",
                Outcome::Push => "🤝 Push",
                Outcome::Blackjack => "⭐ Blackjack!",
                Outcome::Surrender => "🏳 Surrender",
            };
            event_log.push(GameEvent::result(format!(
                "{}: {} → {} chips",
                result.name, outcome_str, result.final_bankroll
            )));
        }

        // Update bankrolls — results are parallel to active_indices, not to
        // bankrolls directly (skipped seats with bankroll==0 are not in results).
        for (seat_idx, result) in active_indices.iter().zip(results.iter()) {
            bankrolls[*seat_idx] = result.final_bankroll;
        }

        // Drain any remaining chat messages.
        while let Ok(msg) = chat_rx.try_recv() {
            chat_messages.push(msg);
        }

        let _ = render_multi(
            terminal,
            MultiRenderCtx {
                seat_names: &seat_names,
                bankrolls: &bankrolls,
                dealer_hand: &dealer_hand_snap,
                seats: &seats_snap,
                active_seat: usize::MAX,
                chat_messages: &chat_messages,
                bj_nodes: &bj_nodes,
                bj_edges: &bj_edges,
                show_typestate_graph,
                event_log: &event_log,
                prompt: None,
            },
            Established::<MultiRoundActive>::assert(),
        )?;

        if bankrolls.iter().all(|&b| b == 0) {
            info!("All bankrolls exhausted — ending session");
            break 'session;
        }

        if !wait_for_keypress().await? {
            break 'session;
        }
    }

    Ok(())
}

/// Context bundle for [`render_multi`] — avoids too-many-arguments lint.
struct MultiRenderCtx<'a> {
    seat_names: &'a [String],
    bankrolls: &'a [u64],
    dealer_hand: &'a Hand,
    seats: &'a [strictly_blackjack::SeatPlay],
    active_seat: usize,
    chat_messages: &'a [ChatMessage],
    bj_nodes: &'a [crate::tui::typestate_widget::NodeDef],
    bj_edges: &'a [crate::tui::typestate_widget::EdgeDef],
    show_typestate_graph: bool,
    event_log: &'a [GameEvent],
    prompt: Option<&'a str>,
}

/// Minimal multi-player render: left=hands, center=typestate+log, right=chat.
#[instrument(skip_all)]
fn render_multi<B: Backend, P: Prop>(
    terminal: &mut Terminal<B>,
    ctx: MultiRenderCtx<'_>,
    game_proof: Established<P>,
) -> Result<Established<And<P, NoOverflow>>>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::tui::ChatWidget;
    use crate::tui::contracts::{NoOverflow, render_resize_prompt, verified_draw};
    use crate::tui::palette::GamePalette;
    use elicit_ratatui::{
        BlockJson, BordersJson, ConstraintJson, DirectionJson, LineJson, ModifierJson,
        ParagraphText, SpanJson, StyleJson, TextJson, TuiNode, WidgetJson,
    };
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::prelude::Widget as _;

    let MultiRenderCtx {
        seat_names,
        bankrolls,
        dealer_hand,
        seats,
        active_seat,
        chat_messages,
        bj_nodes,
        bj_edges,
        show_typestate_graph,
        event_log,
        prompt,
    } = ctx;

    let pal = GamePalette::new();
    let border_style = StyleJson {
        fg: Some(pal.border.json.clone()),
        bg: None,
        modifiers: vec![],
    };
    let muted_style = StyleJson {
        fg: Some(pal.muted.json.clone()),
        bg: None,
        modifiers: vec![],
    };

    // ── Dealer text ────────────────────────────────────────────────────────
    let dealer_str = dealer_hand
        .cards()
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let dealer_value = if dealer_hand.cards().is_empty() {
        String::new()
    } else {
        format!("  ({})", dealer_hand.value().best())
    };
    let dealer_text = TextJson {
        lines: vec![LineJson {
            spans: vec![SpanJson {
                content: format!("  {dealer_str}{dealer_value}"),
                style: Some(StyleJson {
                    fg: Some(pal.title.json.clone()),
                    bg: None,
                    modifiers: vec![],
                }),
            }],
            style: None,
            alignment: None,
        }],
        style: None,
        alignment: None,
    };

    // ── Seats text ─────────────────────────────────────────────────────────
    let mut seat_lines: Vec<LineJson> = Vec::new();
    for (i, seat) in seats.iter().enumerate() {
        let (fg, prefix) = if i == active_seat {
            (pal.success.json.clone(), "► ")
        } else {
            (pal.body.json.clone(), "  ")
        };
        seat_lines.push(LineJson {
            spans: vec![SpanJson {
                content: format!("{prefix}{} (bet {})  ", seat.name, seat.bet),
                style: Some(StyleJson {
                    fg: Some(fg),
                    bg: None,
                    modifiers: vec![ModifierJson::Bold],
                }),
            }],
            style: None,
            alignment: None,
        });
        let cards_str = seat
            .hand
            .cards()
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let value = seat.hand.value().best();
        let status = if seat.natural {
            "★ Natural!".to_string()
        } else if seat.bust {
            "✗ Bust".to_string()
        } else if seat.stood {
            format!("Stand ({value})")
        } else {
            format!("({value})")
        };
        seat_lines.push(LineJson {
            spans: vec![SpanJson {
                content: format!("    {cards_str}  {status}"),
                style: Some(StyleJson {
                    fg: Some(pal.body.json.clone()),
                    bg: None,
                    modifiers: vec![],
                }),
            }],
            style: None,
            alignment: None,
        });
    }
    if seats.is_empty() {
        for (i, name) in seat_names.iter().enumerate() {
            let bankroll = bankrolls.get(i).copied().unwrap_or(0);
            seat_lines.push(LineJson {
                spans: vec![SpanJson {
                    content: format!("  {name}: {bankroll} chips"),
                    style: Some(StyleJson {
                        fg: Some(pal.body.json.clone()),
                        bg: None,
                        modifiers: vec![],
                    }),
                }],
                style: None,
                alignment: None,
            });
        }
    }
    let seats_text = TextJson {
        lines: seat_lines,
        style: None,
        alignment: None,
    };

    // ── Story/log text ─────────────────────────────────────────────────────
    let log_lines: Vec<LineJson> = event_log
        .iter()
        .map(|e| LineJson {
            spans: vec![SpanJson {
                content: e.text.clone(),
                style: Some(StyleJson {
                    fg: Some(crate::tui::ratatui_color_to_json(e.color, &pal)),
                    bg: None,
                    modifiers: vec![],
                }),
            }],
            style: None,
            alignment: None,
        })
        .collect();
    let log_text = TextJson {
        lines: log_lines,
        style: None,
        alignment: None,
    };

    // ── Assemble TuiNode tree ──────────────────────────────────────────────
    let dealer_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(dealer_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Dealer ".to_string()),
                style: None,
                border_style: Some(border_style.clone()),
                padding: None,
            }),
        }),
    };
    let seats_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(seats_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Seats ".to_string()),
                style: None,
                border_style: Some(border_style.clone()),
                padding: None,
            }),
        }),
    };
    let center_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(log_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Game Log ".to_string()),
                style: None,
                border_style: Some(border_style.clone()),
                padding: None,
            }),
        }),
    };

    let root = TuiNode::Layout {
        direction: DirectionJson::Vertical,
        constraints: vec![
            ConstraintJson::Min { value: 0 },
            ConstraintJson::Length {
                value: PROMPT_PANE_HEIGHT,
            },
        ],
        children: vec![
            TuiNode::Layout {
                direction: DirectionJson::Horizontal,
                constraints: vec![
                    ConstraintJson::Percentage { value: 40 },
                    ConstraintJson::Percentage { value: 35 },
                    ConstraintJson::Percentage { value: 25 },
                ],
                children: vec![
                    TuiNode::Layout {
                        direction: DirectionJson::Vertical,
                        constraints: vec![
                            ConstraintJson::Length { value: 4 },
                            ConstraintJson::Min { value: 0 },
                        ],
                        children: vec![dealer_node, seats_node],
                        margin: None,
                    },
                    center_node,
                    TuiNode::Widget {
                        widget: Box::new(WidgetJson::Clear),
                    },
                ],
                margin: None,
            },
            TuiNode::Widget {
                widget: Box::new(WidgetJson::Paragraph {
                    text: ParagraphText::Plain(String::new()),
                    style: None,
                    wrap: false,
                    scroll: None,
                    alignment: None,
                    block: Some(BlockJson {
                        borders: BordersJson::All,
                        border_type: None,
                        title: Some(" Input ".to_string()),
                        style: None,
                        border_style: Some(muted_style),
                        padding: None,
                    }),
                }),
            },
        ],
        margin: None,
    };

    terminal.draw(|frame| {
        let _proof: Established<NoOverflow> = verified_draw(frame, frame.area(), &root)
            .unwrap_or_else(|e| {
                render_resize_prompt(frame, &e);
                Established::assert()
            });

        // Compute layout to find areas for custom widgets.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(PROMPT_PANE_HEIGHT)])
            .split(frame.area());
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(35),
                Constraint::Percentage(25),
            ])
            .split(outer[0]);

        // Center: typestate graph (if enabled) or log already rendered above.
        if show_typestate_graph {
            let phase_ctx =
                PhaseContext::info("Playing…").with_pending_prompt(prompt.map(|s| s.to_string()));
            TypestateGraphWidget::new(
                bj_nodes,
                bj_edges,
                blackjack_active("PlayerTurn"),
                event_log,
            )
            .with_context(&phase_ctx)
            .render(cols[1], frame.buffer_mut());
        }

        // Right: chat.
        let (chat, _chat_proof) = ChatWidget::new(chat_messages);
        chat.render(cols[2], frame.buffer_mut());
    })?;
    Ok(both(game_proof, Established::assert()))
}
async fn wait_for_keypress() -> Result<bool> {
    use crossterm::{
        cursor::MoveTo,
        execute,
        style::{Color, Print, ResetColor, SetForegroundColor},
    };

    // Show instruction in the prompt pane so the user knows to press a key.
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let content_row = rows.saturating_sub(PROMPT_PANE_HEIGHT) + 1;
    let content_left: u16 = 2;
    let mut stdout = std::io::stdout();
    // Clear interior of pane.
    for row in content_row..rows.saturating_sub(1) {
        execute!(
            stdout,
            MoveTo(1, row),
            Print(" ".repeat(cols.saturating_sub(2) as usize))
        )
        .ok();
    }
    execute!(
        stdout,
        MoveTo(content_left, content_row),
        SetForegroundColor(Color::Yellow),
        Print("Press any key to continue (q to quit)..."),
        ResetColor,
    )
    .ok();
    let _ = std::io::Write::flush(&mut stdout);

    // Drain any events buffered during the previous input phase (e.g. the
    // Enter key from the bet prompt) before we start watching for fresh input.
    sleep(Duration::from_millis(100)).await;
    while event::poll(std::time::Duration::ZERO)? {
        let _ = event::read()?;
    }

    loop {
        sleep(Duration::from_millis(50)).await;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(k) = event::read()?
            && k.code != KeyCode::Null
        {
            // Drain any remaining buffered events so they don't leak into
            // the next elicitation prompt.
            sleep(Duration::from_millis(50)).await;
            while event::poll(std::time::Duration::ZERO)? {
                let _ = event::read()?;
            }

            let quit = matches!(k.code, KeyCode::Char('q') | KeyCode::Char('Q'));
            return Ok(!quit);
        }
    }
}

// ── MCP spectator session ────────────────────────────────────────────────────

/// Run a blackjack session driven by an MCP agent subprocess.
///
/// Spawns the HTTP game server and the agent subprocess, then runs a
/// spectator render loop.  The human player watches the agent play —
/// there is no interactive input except `q` to quit.
///
/// Polls `/api/sessions/tui_session/blackjack_state` and
/// `/api/sessions/tui_session/dialogue` to keep the TUI up to date.
#[instrument(skip_all, fields(port, player_name = %player_name, initial_bankroll, show_typestate_graph))]
pub async fn run_blackjack_mcp_session<B: Backend>(
    terminal: &mut Terminal<B>,
    agent_config_path: std::path::PathBuf,
    player_name: String,
    port: u16,
    initial_bankroll: u64,
    show_typestate_graph: bool,
) -> Result<BlackjackSessionOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::session::DialogueEntry;
    use crate::tui::chat_widget::ChatWidget;
    use crate::tui::rest_client::BlackjackObserver;
    use crate::tui::standalone::{GameMode, ProcessGuards, spawn_agent, spawn_server};
    use crate::tui::typestate_widget::{TypestateGraphWidget, blackjack_active};
    use crate::tui::{ChatMessage, Participant};
    use crossterm::event::{Event, KeyCode};
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::prelude::Widget;
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    info!("Starting MCP blackjack spectator session");

    let server_url = format!("http://localhost:{}", port);

    let server = spawn_server(port).await?;
    let agent = spawn_agent(
        port,
        agent_config_path,
        GameMode::Blackjack {
            bankroll: initial_bankroll,
        },
    )
    .await?;
    let _guards = ProcessGuards::new(server, agent);

    let observer = BlackjackObserver::new(server_url, "tui_session".to_string());

    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();

    let mut dialogue: Vec<DialogueEntry> = Vec::new();
    let mut last_is_terminal = false;

    loop {
        // Poll game state
        let state = observer.get_blackjack_state().await.unwrap_or_else(|_| {
            crate::games::blackjack::BlackjackStateView {
                phase: "idle".to_string(),
                bankroll: 0,
                description: "Connecting...".to_string(),
                is_terminal: false,
            }
        });

        if let Ok(entries) = observer.get_dialogue().await {
            dialogue = entries;
        }

        let is_terminal = state.is_terminal;
        let phase_name = state.phase.clone();
        let description = state.description.clone();
        let bankroll = state.bankroll;

        // Convert dialogue to ChatMessages
        let messages: Vec<ChatMessage> = dialogue
            .iter()
            .map(|e| {
                let participant = if e.role == "Agent" {
                    Participant::Agent("Agent".to_string())
                } else {
                    Participant::Host
                };
                ChatMessage::new(participant, e.text.clone())
            })
            .collect();

        terminal.draw(|f| {
            let area = f.area();

            // Header
            let title = format!(
                " 🎰 Blackjack — {} watching {} | Bankroll: ${} ",
                player_name,
                if phase_name == "idle" {
                    "Idle"
                } else {
                    "Agent"
                },
                bankroll
            );

            let outer = Block::default()
                .title(title.as_str())
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow));
            let inner = outer.inner(area);
            outer.render(area, f.buffer_mut());

            // Split into left (game + chat) and right (typestate graph)
            let (left_area, right_area) = if show_typestate_graph {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(inner);
                (chunks[0], Some(chunks[1]))
            } else {
                (inner, None)
            };

            // Split left into game state (top) and chat (bottom)
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(left_area);

            let game_area = left_chunks[0];
            let chat_area = left_chunks[1];

            // Game state pane
            let game_block = Block::default()
                .title(format!(" Phase: {} ", phase_name))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan));
            let game_paragraph = Paragraph::new(description.as_str())
                .block(game_block)
                .wrap(ratatui::widgets::Wrap { trim: false });
            game_paragraph.render(game_area, f.buffer_mut());

            // Chat pane
            let (chat_widget, _proof) = ChatWidget::new(&messages);
            chat_widget.render(chat_area, f.buffer_mut());

            // Typestate graph
            if let Some(ts_area) = right_area {
                let active_idx = blackjack_active(&phase_name);
                let widget = TypestateGraphWidget::new(&bj_nodes, &bj_edges, active_idx, &[]);
                widget.render(ts_area, f.buffer_mut());
            }
        })?;

        // Handle terminal session end
        if is_terminal && !last_is_terminal {
            last_is_terminal = true;
        }
        if is_terminal {
            // Wait for keypress then exit
            tokio::time::sleep(Duration::from_millis(50)).await;
            if crossterm::event::poll(std::time::Duration::from_millis(100))? {
                let ev = crossterm::event::read()?;
                if let Event::Key(_) = ev {
                    return Ok(BlackjackSessionOutcome::Abandoned);
                }
            }
        }

        // Check for quit key
        if crossterm::event::poll(std::time::Duration::ZERO)?
            && let Event::Key(k) = crossterm::event::read()?
            && matches!(k.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        {
            return Ok(BlackjackSessionOutcome::Abandoned);
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
