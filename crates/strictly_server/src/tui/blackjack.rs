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

use crate::tui::contracts::NoOverflow;
use crate::tui::contracts::{BettingActive, MultiRoundActive};
use crate::tui::observable_communicator::ObservableCommunicator;
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    ChoiceHint, GameEvent, PhaseContext, TypestateGraphWidget, blackjack_active, blackjack_edges,
    blackjack_nodes,
};
use crate::tui::{ChatMessage, LlmElicitCommunicator, Participant, chat_channel};
use crate::{PlayerKind, PlayerSlot};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use elicitation::ElicitCommunicator as _;
use elicitation::Elicitation as _;
use elicitation::contracts::{And, Established, Prop, both};
use ratatui::{Terminal, backend::Backend, prelude::Widget};
use std::time::SystemTime;
use strictly_blackjack::{
    BasicAction, BetPlaced, BlackjackAction, BlackjackPlayerView, GameBetting, GameFinished,
    GamePlayerTurn, GameSetup, Hand, MultiRound, Outcome, PayoutSettled, PlaceBetOutput,
    PlayActionOutput, PlayActionResult, SeatBet, execute_dealer_turn, execute_place_bet,
    execute_play_action,
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

/// Current phase rendered in the TUI display.
#[derive(Debug)]
enum DisplayPhase<'a> {
    Betting { state: &'a GameBetting },
    PlayerTurn { state: &'a GamePlayerTurn },
    Finished { state: &'a GameFinished },
}

/// Shared rendering context threaded through all render calls in a session.
struct RenderCtx<'a, B: Backend> {
    terminal: &'a mut Terminal<B>,
    player_name: &'a str,
    show_typestate_graph: bool,
    bj_nodes: &'a [crate::tui::typestate_widget::NodeDef],
    bj_edges: &'a [crate::tui::typestate_widget::EdgeDef],
    /// Receives the latest in-flight prompt from [`ObservableCommunicator`].
    /// `None` when no elicitation is active.
    prompt_rx: watch::Receiver<Option<String>>,
}

// ─────────────────────────────────────────────────────────────
//  Public entry point
// ─────────────────────────────────────────────────────────────

/// Run a complete blackjack session in the TUI (multi-round).
///
/// The game is entirely local (no REST server). Player decisions are elicited
/// via [`TuiCommunicator`]. Each hand uses proof-carrying workflow tools to
/// enforce correct phase transitions at compile time. After each hand the
/// player may choose to play again (unless their bankroll hits zero).
///
/// Returns the session outcome so the caller can record stats and return to
/// the lobby.
#[instrument(skip_all, fields(player_name = %player_name, initial_bankroll, show_typestate_graph))]
pub async fn run_blackjack_session<B: Backend>(
    terminal: &mut Terminal<B>,
    player_name: String,
    initial_bankroll: u64,
    show_typestate_graph: bool,
) -> Result<BlackjackSessionOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    info!("Starting blackjack session");

    let (prompt_tx, prompt_rx) = watch::channel(None::<String>);
    let comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx);
    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();
    let mut bankroll = initial_bankroll;
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut last_outcome;

    loop {
        event_log.push(GameEvent::story(format!("🂠  New hand — {bankroll} chips")));

        let mut ctx = RenderCtx {
            terminal,
            player_name: &player_name,
            show_typestate_graph,
            bj_nodes: &bj_nodes,
            bj_edges: &bj_edges,
            prompt_rx: prompt_rx.clone(),
        };

        let Some((hand_result, hand_outcome, settled_proof)) =
            run_single_hand(&mut ctx, bankroll, &comm, &mut event_log).await?
        else {
            return Ok(BlackjackSessionOutcome::Abandoned);
        };

        bankroll = hand_result.bankroll();
        last_outcome = hand_outcome;

        let mut ctx = RenderCtx {
            terminal,
            player_name: &player_name,
            show_typestate_graph,
            bj_nodes: &bj_nodes,
            bj_edges: &bj_edges,
            prompt_rx: prompt_rx.clone(),
        };
        let _ = render_finish(
            &mut ctx,
            &hand_result,
            &last_outcome,
            &event_log,
            settled_proof,
        )?;

        // Q = quit, any other key = play another hand.
        if !wait_for_keypress().await? || bankroll == 0 {
            if bankroll == 0 {
                info!("Bankroll exhausted — ending session");
            }
            break;
        }
    }

    Ok(last_outcome)
}

// ─────────────────────────────────────────────────────────────
//  Single hand
// ─────────────────────────────────────────────────────────────

/// Runs one complete hand using proof-carrying workflow tools.
///
/// Returns `Some((finished_state, session_outcome))`, or `None` if the player
/// abandoned the session mid-hand.
#[instrument(skip_all, fields(bankroll))]
async fn run_single_hand<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    bankroll: u64,
    comm: &ObservableCommunicator<TuiCommunicator>,
    event_log: &mut Vec<GameEvent>,
) -> Result<
    Option<(
        GameFinished,
        BlackjackSessionOutcome,
        Established<PayoutSettled>,
    )>,
>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let mut current_phase = "Betting".to_string();

    // ── Betting phase ─────────────────────────────────────────
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let game = GameSetup::new(seed);
    let betting = game.start_betting(bankroll);

    let _ = render_blackjack(
        ctx,
        DisplayPhase::Betting { state: &betting },
        blackjack_active(&current_phase),
        event_log,
        Established::<BettingActive>::assert(),
    )?;

    let bet = loop {
        let styled =
            comm.with_style::<u64, BlackjackBetStyle>(BlackjackBetStyle::new(betting.bankroll()));
        let raw: u64 = match u64::elicit(&styled).await {
            Ok(v) => v,
            Err(_) => {
                warn!("Elicitation cancelled during betting");
                return Ok(None);
            }
        };
        if raw == 0 || raw > betting.bankroll() {
            continue;
        }
        break raw;
    };

    // ── execute_place_bet (True → BetPlaced | PayoutSettled) ──
    let place_output = execute_place_bet(betting, bet).map_err(anyhow::Error::msg)?;

    event_log.push(GameEvent::story(format!("💰  Bet {bet} — cards dealt")));

    let (finished, settled_proof) = match place_output {
        // Natural blackjack / dealer natural — no player actions needed.
        PlaceBetOutput::Finished(f, settled) => {
            let reason =
                if f.dealer_hand().is_blackjack() && f.outcomes().iter().any(|o| o.is_loss()) {
                    "⚡  Dealer natural blackjack"
                } else if f.player_hands().first().is_some_and(|h| h.is_blackjack()) {
                    "⚡  Natural blackjack — 3:2 payout!"
                } else {
                    "⚡  Instant resolution"
                };
            event_log.push(GameEvent::story(reason));
            event_log.push(GameEvent::phase_change("Betting", "Finished"));
            event_log.push(GameEvent::proof("PayoutSettled"));
            (f, settled)
        }
        // Normal play — enter player action loop.
        PlaceBetOutput::PlayerTurn(pt, bet_proof) => {
            current_phase = "PlayerTurn".to_string();
            event_log.push(GameEvent::phase_change("Betting", "PlayerTurn"));
            event_log.push(GameEvent::proof("BetPlaced"));
            let player_val = pt
                .player_hands()
                .first()
                .map(|h| h.value().best().to_string())
                .unwrap_or_default();
            let upcard = pt
                .dealer_hand()
                .cards()
                .first()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string());
            event_log.push(GameEvent::story(format!(
                "🂠  You have {player_val} · Dealer shows {upcard}"
            )));
            play_player_turn(ctx, pt, bet_proof, comm, &mut current_phase, event_log).await?
        }
    };

    let outcome = compute_outcome(&finished);
    let outcome_story = match outcome {
        BlackjackSessionOutcome::Win(b) => format!("🎉  Won! Bankroll → {b}"),
        BlackjackSessionOutcome::Loss(b) => format!("💸  Lost. Bankroll → {b}"),
        BlackjackSessionOutcome::Push(b) => format!("🤝  Push. Bankroll → {b}"),
        BlackjackSessionOutcome::Abandoned => "Session ended".to_string(),
    };
    event_log.push(GameEvent::result(outcome_story));

    Ok(Some((finished, outcome, settled_proof)))
}

// ─────────────────────────────────────────────────────────────
//  Player turn loop (BetPlaced → PlayerTurnComplete → PayoutSettled)
// ─────────────────────────────────────────────────────────────

#[instrument(skip_all)]
async fn play_player_turn<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    mut state: GamePlayerTurn,
    mut current_proof: Established<BetPlaced>,
    comm: &ObservableCommunicator<TuiCommunicator>,
    current_phase: &mut String,
    event_log: &mut Vec<GameEvent>,
) -> Result<(GameFinished, Established<PayoutSettled>)>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        let _ = render_blackjack(
            ctx,
            DisplayPhase::PlayerTurn { state: &state },
            blackjack_active(current_phase),
            event_log,
            current_proof,
        )?;

        let action = match BasicAction::elicit(comm).await {
            Ok(a) => a,
            Err(_) => {
                warn!("Elicitation cancelled during player turn — standing");
                BasicAction::Stand
            }
        };

        event_log.push(GameEvent::story(format!(
            "▸  {}",
            match action {
                BasicAction::Hit => "Hit",
                BasicAction::Stand => "Stand",
            }
        )));

        // Capture hand value before state is consumed by execute_play_action.
        let pre_action_hand_val = state
            .player_hands()
            .get(state.current_hand_index())
            .map(|h: &Hand| h.value().best().to_string())
            .unwrap_or_default();

        // ── execute_play_action (BetPlaced → BetPlaced | PlayerTurnComplete) ──
        match execute_play_action(state, action, current_proof).map_err(anyhow::Error::msg)? {
            PlayActionResult::InProgress(next, proof) => {
                // Show new hand value after hit.
                let new_val = next
                    .player_hands()
                    .get(next.current_hand_index())
                    .map(|h| h.value().best().to_string())
                    .unwrap_or_default();
                event_log.push(GameEvent::story(format!("   hand now {new_val}")));
                state = next;
                current_proof = proof;
            }
            PlayActionResult::Complete(output, player_done_proof) => {
                match output {
                    PlayActionOutput::Finished(f) => {
                        *current_phase = "Finished".to_string();
                        // Check if bust.
                        let bust = f.player_hands().first().is_some_and(|h| h.is_bust());
                        if bust {
                            event_log.push(GameEvent::story("   bust!".to_string()));
                        }
                        event_log.push(GameEvent::phase_change("PlayerTurn", "Finished"));
                        event_log.push(GameEvent::proof("PayoutSettled"));
                        return Ok((f, Established::assert()));
                    }
                    PlayActionOutput::DealerTurn(dt) => {
                        *current_phase = "DealerTurn".to_string();
                        event_log.push(GameEvent::story(format!(
                            "   stood at {pre_action_hand_val} — dealer's turn"
                        )));
                        event_log.push(GameEvent::phase_change("PlayerTurn", "DealerTurn"));
                        event_log.push(GameEvent::proof("PlayerTurnComplete"));

                        // ── execute_dealer_turn (PlayerTurnComplete → PayoutSettled) ──
                        let (finished, settled_proof) = execute_dealer_turn(dt, player_done_proof);

                        // Narrate dealer result.
                        let dealer_val = finished.dealer_hand().value().best();
                        let dealer_story = if finished.dealer_hand().is_bust() {
                            format!("🎲  Dealer bust ({dealer_val}) — you win!")
                        } else {
                            format!("🎲  Dealer stands at {dealer_val}")
                        };
                        event_log.push(GameEvent::story(dealer_story));
                        event_log.push(GameEvent::phase_change("DealerTurn", "Finished"));
                        event_log.push(GameEvent::proof("PayoutSettled"));

                        *current_phase = "Finished".to_string();
                        return Ok((finished, settled_proof));
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Outcome helpers
// ─────────────────────────────────────────────────────────────

fn compute_outcome(finished: &GameFinished) -> BlackjackSessionOutcome {
    let bankroll = finished.bankroll();
    let any_win = finished.outcomes().iter().any(|&o| Outcome::is_win(o));
    let any_loss = finished.outcomes().iter().any(|&o| Outcome::is_loss(o));

    if any_win {
        BlackjackSessionOutcome::Win(bankroll)
    } else if any_loss {
        BlackjackSessionOutcome::Loss(bankroll)
    } else {
        BlackjackSessionOutcome::Push(bankroll)
    }
}

// ─────────────────────────────────────────────────────────────
//  Rendering helpers
// ─────────────────────────────────────────────────────────────

#[instrument(skip_all)]
fn render_blackjack<B: Backend, P: Prop>(
    ctx: &mut RenderCtx<'_, B>,
    phase: DisplayPhase<'_>,
    active: Option<usize>,
    event_log: &[GameEvent],
    game_proof: Established<P>,
) -> Result<Established<And<P, NoOverflow>>>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::tui::contracts::{NoOverflow, render_resize_prompt, verified_draw};
    use crate::tui::palette::GamePalette;
    use elicit_ratatui::{
        BlockJson, BordersJson, ConstraintJson, DirectionJson, ParagraphText, StyleJson, TuiNode,
        WidgetJson,
    };
    use ratatui::layout::{Constraint, Direction, Layout};

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

    // Snapshot the latest in-flight prompt (non-blocking).
    let pending_prompt = ctx.prompt_rx.borrow().clone();
    let phase_ctx = build_phase_context(&phase).with_pending_prompt(pending_prompt);

    let game_title = format!(" ♠ Blackjack — {} ♣ ", ctx.player_name);
    let game_text = build_game_text(&phase, &pal);

    let content_constraints: Vec<ConstraintJson> = if ctx.show_typestate_graph {
        vec![
            ConstraintJson::Percentage { value: 55 },
            ConstraintJson::Percentage { value: 45 },
        ]
    } else {
        vec![ConstraintJson::Percentage { value: 100 }]
    };

    // Build the game panel node (always present).
    let game_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(game_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(game_title),
                style: None,
                border_style: Some(border_style.clone()),
                padding: None,
            }),
        }),
    };

    // Typestate placeholder (Clear) — custom widget renders into this area.
    let mut content_children = vec![game_node];
    if ctx.show_typestate_graph {
        content_children.push(TuiNode::Widget {
            widget: Box::new(WidgetJson::Clear),
        });
    }

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
                constraints: content_constraints,
                children: content_children,
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

    ctx.terminal.draw(|frame| {
        let _proof: Established<NoOverflow> = verified_draw(frame, frame.area(), &root)
            .unwrap_or_else(|e| {
                render_resize_prompt(frame, &e);
                Established::assert()
            });

        // Compute layout to find the typestate graph area.
        if ctx.show_typestate_graph {
            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(0), Constraint::Length(PROMPT_PANE_HEIGHT)])
                .split(frame.area());
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(outer[0]);
            TypestateGraphWidget::new(ctx.bj_nodes, ctx.bj_edges, active, event_log)
                .with_context(&phase_ctx)
                .render(cols[1], frame.buffer_mut());
        }
    })?;
    Ok(both(game_proof, Established::assert()))
}

#[instrument(skip_all)]
fn render_finish<B: Backend, P: Prop>(
    ctx: &mut RenderCtx<'_, B>,
    finished: &GameFinished,
    outcome: &BlackjackSessionOutcome,
    event_log: &[GameEvent],
    game_proof: Established<P>,
) -> Result<Established<And<P, NoOverflow>>>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::tui::contracts::{NoOverflow, render_resize_prompt, verified_draw};
    use crate::tui::palette::GamePalette;
    use elicit_ratatui::{
        BlockJson, BordersJson, LineJson, ModifierJson, ParagraphText, SpanJson, StyleJson,
        TextJson, TuiNode, WidgetJson,
    };

    let _ = render_blackjack(
        ctx,
        DisplayPhase::Finished { state: finished },
        blackjack_active("Finished"),
        event_log,
        game_proof,
    )?;

    let pal = GamePalette::new();
    let outcome_text = match outcome {
        BlackjackSessionOutcome::Win(b) => format!("🎉  You WIN! Bankroll: {b}"),
        BlackjackSessionOutcome::Loss(b) => format!("💸  You lose. Bankroll: {b}"),
        BlackjackSessionOutcome::Push(b) => format!("🤝  Push. Bankroll: {b}"),
        BlackjackSessionOutcome::Abandoned => "Session ended.".to_string(),
    };

    let popup_text = TextJson {
        lines: vec![LineJson {
            spans: vec![
                SpanJson {
                    content: outcome_text.clone(),
                    style: Some(StyleJson {
                        fg: Some(pal.warning.json.clone()),
                        bg: None,
                        modifiers: vec![ModifierJson::Bold],
                    }),
                },
                SpanJson {
                    content: "  — press any key to continue".to_string(),
                    style: Some(StyleJson {
                        fg: Some(pal.body.json.clone()),
                        bg: None,
                        modifiers: vec![],
                    }),
                },
            ],
            style: None,
            alignment: None,
        }],
        style: None,
        alignment: None,
    };

    let popup_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(popup_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Result ".to_string()),
                style: None,
                border_style: Some(StyleJson {
                    fg: Some(pal.border.json.clone()),
                    bg: None,
                    modifiers: vec![],
                }),
                padding: None,
            }),
        }),
    };

    ctx.terminal.draw(|frame| {
        let area = frame.area();
        let w = (area.width as usize).min(outcome_text.len() + 44) as u16;
        let h = 3u16;
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let rect = ratatui::layout::Rect {
            x,
            y,
            width: w,
            height: h,
        };
        let _proof: Established<NoOverflow> = verified_draw(frame, rect, &popup_node)
            .unwrap_or_else(|e| {
                render_resize_prompt(frame, &e);
                Established::assert()
            });
    })?;
    Ok(both(game_proof, Established::assert()))
}

/// Builds the [`PhaseContext`] callout from the current display phase.
///
/// Called once per render; the resulting context is passed to the typestate
/// widget so it can show the narrative branch beneath the active node.
fn build_phase_context(phase: &DisplayPhase<'_>) -> PhaseContext {
    match phase {
        DisplayPhase::Betting { state } => PhaseContext::info(format!(
            "You have {} chips — enter your bet",
            state.bankroll()
        )),

        DisplayPhase::PlayerTurn { state } => {
            let hand = state
                .player_hands()
                .get(state.current_hand_index())
                .map(|h| h.value().best().to_string())
                .unwrap_or_else(|| "?".to_string());
            let upcard = state
                .dealer_hand()
                .cards()
                .first()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string());
            PhaseContext::with_choices(
                format!("Your {hand} vs dealer {upcard}"),
                vec![
                    ChoiceHint {
                        key: "1",
                        label: "Hit",
                        desc: "draw another card",
                    },
                    ChoiceHint {
                        key: "2",
                        label: "Stand",
                        desc: "hold, let dealer play",
                    },
                ],
            )
        }

        DisplayPhase::Finished { state } => {
            let any_win = state.outcomes().iter().any(|o| o.is_win());
            let narrative = if any_win {
                "Hand complete — you won this round".to_string()
            } else if state.outcomes().iter().any(|o| o.is_loss()) {
                "Hand complete — better luck next time".to_string()
            } else {
                "Hand complete — push, bet returned".to_string()
            };
            PhaseContext::info(narrative)
        }
    }
}

fn build_game_text(
    phase: &DisplayPhase<'_>,
    pal: &crate::tui::palette::GamePalette,
) -> elicit_ratatui::TextJson {
    use elicit_ratatui::{LineJson, ModifierJson, SpanJson, StyleJson, TextJson};

    let warning_style = StyleJson {
        fg: Some(pal.warning.json.clone()),
        bg: None,
        modifiers: vec![ModifierJson::Bold],
    };
    let muted_style = StyleJson {
        fg: Some(pal.muted.json.clone()),
        bg: None,
        modifiers: vec![],
    };
    let body_style = StyleJson {
        fg: Some(pal.body.json.clone()),
        bg: None,
        modifiers: vec![],
    };
    let highlight_style = StyleJson {
        fg: Some(pal.highlight.json.clone()),
        bg: None,
        modifiers: vec![ModifierJson::Bold],
    };
    let success_style = StyleJson {
        fg: Some(pal.success.json.clone()),
        bg: None,
        modifiers: vec![ModifierJson::Bold],
    };
    let error_style = StyleJson {
        fg: Some(pal.error.json.clone()),
        bg: None,
        modifiers: vec![ModifierJson::Bold],
    };

    let plain = |s: String| LineJson {
        spans: vec![SpanJson {
            content: s,
            style: Some(body_style.clone()),
        }],
        style: None,
        alignment: None,
    };
    let empty = || LineJson {
        spans: vec![SpanJson {
            content: String::new(),
            style: None,
        }],
        style: None,
        alignment: None,
    };
    let styled_line = |s: String, st: StyleJson| LineJson {
        spans: vec![SpanJson {
            content: s,
            style: Some(st),
        }],
        style: None,
        alignment: None,
    };

    let mut lines: Vec<LineJson> = Vec::new();

    match phase {
        DisplayPhase::Betting { state } => {
            lines.push(styled_line("♦  Place your bet".to_string(), warning_style));
            lines.push(empty());
            lines.push(plain(format!("  Bankroll: {} chips", state.bankroll())));
            lines.push(empty());
            lines.push(styled_line(
                "  (Enter amount below ↓)".to_string(),
                muted_style,
            ));
        }
        DisplayPhase::PlayerTurn { state } => {
            let dealer_cards = state.dealer_hand().cards();
            let dealer_str = if dealer_cards.is_empty() {
                "  Dealer: [?]".to_string()
            } else {
                let visible = dealer_cards
                    .first()
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                format!("  Dealer: {visible} [?]  (hit 17+)")
            };
            lines.push(plain(dealer_str));
            lines.push(empty());

            for (i, hand) in state.player_hands().iter().enumerate() {
                let is_active = i == state.current_hand_index();
                let marker = if is_active { "▶" } else { " " };
                let st = if is_active {
                    highlight_style.clone()
                } else {
                    muted_style.clone()
                };
                lines.push(styled_line(format!("  {marker} Your hand: {hand}"), st));
            }
            lines.push(empty());
            lines.push(styled_line(
                "  (Choose action below ↓)".to_string(),
                muted_style,
            ));
        }
        DisplayPhase::Finished { state } => {
            let dealer_hand = state.dealer_hand();
            let dealer_natural = dealer_hand.is_blackjack();
            let player_natural = state
                .player_hands()
                .first()
                .is_some_and(|h| h.is_blackjack());

            let (banner, banner_st) = match (player_natural, dealer_natural) {
                (true, true) => (
                    Some("  ♦ Both have natural blackjack — Push!"),
                    warning_style.clone(),
                ),
                (true, false) => (
                    Some("  ♠ Natural blackjack! 3:2 payout"),
                    success_style.clone(),
                ),
                (false, true) => (Some("  ♦ Dealer natural blackjack"), error_style.clone()),
                (false, false) => (None, body_style.clone()),
            };
            if let Some(b) = banner {
                lines.push(styled_line(b.to_string(), banner_st));
                lines.push(empty());
            }

            lines.push(plain(format!("  Dealer: {dealer_hand}")));
            lines.push(empty());

            for (i, (hand, outcome)) in state
                .player_hands()
                .iter()
                .zip(state.outcomes().iter())
                .enumerate()
            {
                let bet = state.bets().get(i).copied().unwrap_or(0);
                let bust_note = if hand.is_bust() { "  BUST" } else { "" };
                let st = if outcome.is_win() {
                    success_style.clone()
                } else if outcome.is_loss() {
                    error_style.clone()
                } else {
                    warning_style.clone()
                };
                lines.push(styled_line(
                    format!(
                        "  Hand {}: {}  [{outcome}]{bust_note}  (bet: {bet})",
                        i + 1,
                        hand
                    ),
                    st,
                ));
            }
            lines.push(empty());
            lines.push(plain(format!(
                "  Final bankroll: {} chips",
                state.bankroll()
            )));
        }
    }

    TextJson {
        lines,
        style: None,
        alignment: None,
    }
}

// ─────────────────────────────────────────────────────────────
//  Multi-player helpers
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
///
/// Replaces the generic "Please enter a u64:" with a game-specific prompt
/// so AI agents can understand the expected input.
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
///
/// Uses a custom [`BlackjackBetStyle`] so AI agents see a game-specific prompt
/// instead of the generic "Please enter a u64:".
///
/// Returns `None` only if the communicator signals a cancellation.
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
///
/// Returns `None` only if the communicator signals a cancellation.
#[instrument(skip(comm))]
async fn elicit_action_from<C: elicitation::ElicitCommunicator>(comm: &C) -> Option<BasicAction> {
    BasicAction::elicit(comm).await.ok()
}

/// Elicits a commit action from an agent, allowing exploration of game state.
///
/// Agents see both commit (Hit / Stand) and explore (ViewHand, etc.)
/// variants via [`BlackjackAction`].  When the agent selects an explore
/// variant the corresponding [`BlackjackPlayerView`] category is formatted
/// and cached in a [`KnowledgeCache`] that is prepended to every subsequent
/// prompt.  This way the agent sees all previously gathered knowledge in
/// each new elicitation request.  The loop ends when the agent commits.
#[instrument(skip(comm, round, event_log))]
async fn elicit_agent_action<C: elicitation::ElicitCommunicator + Clone>(
    comm: &C,
    round: &MultiRound,
    seat_idx: usize,
    bankroll: u64,
    event_log: &mut Vec<GameEvent>,
) -> Option<BasicAction> {
    use crate::tui::contextual_communicator::{ContextualCommunicator, knowledge_cache};

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

        // Add to growing knowledge cache so agent sees everything it
        // has learned in every subsequent prompt.
        knowledge
            .lock()
            .unwrap()
            .push(format!("[{category}] {description}"));

        let _ = ctx_comm
            .send_prompt(&format!("[Game State — {category}] {description}"))
            .await;
    }
}

/// Runs a multi-player blackjack session with one human and zero or more AI agents.
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
        let chat = ChatWidget::new(chat_messages);
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
