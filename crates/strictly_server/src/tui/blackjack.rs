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
use elicitation::contracts::Established;
use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    prelude::Widget,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use strictly_blackjack::{
    BasicAction, BetPlaced, GameBetting, GameFinished, GamePlayerTurn, GameSetup, Hand, Outcome,
    PlaceBetOutput, PlayActionOutput, PlayActionResult, SeatBet, execute_dealer_turn,
    execute_place_bet, execute_play_action,
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

        let Some((hand_result, hand_outcome)) =
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
        render_finish(&mut ctx, &hand_result, &last_outcome, &event_log)?;

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
) -> Result<Option<(GameFinished, BlackjackSessionOutcome)>>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let mut current_phase = "Betting".to_string();

    // ── Betting phase ─────────────────────────────────────────
    let game = GameSetup::new();
    let betting = game.start_betting(bankroll);

    render_blackjack(
        ctx,
        DisplayPhase::Betting { state: &betting },
        blackjack_active(&current_phase),
        event_log,
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

    let finished = match place_output {
        // Natural blackjack / dealer natural — no player actions needed.
        PlaceBetOutput::Finished(f, _settled) => {
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
            f
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

    Ok(Some((finished, outcome)))
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
) -> Result<GameFinished>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        render_blackjack(
            ctx,
            DisplayPhase::PlayerTurn { state: &state },
            blackjack_active(current_phase),
            event_log,
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
                        return Ok(f);
                    }
                    PlayActionOutput::DealerTurn(dt) => {
                        *current_phase = "DealerTurn".to_string();
                        event_log.push(GameEvent::story(format!(
                            "   stood at {pre_action_hand_val} — dealer's turn"
                        )));
                        event_log.push(GameEvent::phase_change("PlayerTurn", "DealerTurn"));
                        event_log.push(GameEvent::proof("PlayerTurnComplete"));

                        // ── execute_dealer_turn (PlayerTurnComplete → PayoutSettled) ──
                        let (finished, _resolved) = execute_dealer_turn(dt, player_done_proof);

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
                        return Ok(finished);
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
fn render_blackjack<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    phase: DisplayPhase<'_>,
    active: Option<usize>,
    event_log: &[GameEvent],
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    // Snapshot the latest in-flight prompt (non-blocking).
    let pending_prompt = ctx.prompt_rx.borrow().clone();
    let phase_ctx = build_phase_context(&phase).with_pending_prompt(pending_prompt);

    ctx.terminal.draw(|frame| {
        let full = frame.area();

        // Split: game content on top, dedicated prompt pane at bottom.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(PROMPT_PANE_HEIGHT)])
            .split(full);
        let area = outer[0];
        let prompt_area = outer[1];

        let main_chunks = if ctx.show_typestate_graph {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)])
                .split(area)
        };

        let game_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" ♠ Blackjack — {} ♣ ", ctx.player_name))
            .style(Style::default().fg(Color::White));

        let game_inner = game_block.inner(main_chunks[0]);
        game_block.render(main_chunks[0], frame.buffer_mut());

        let content_lines = build_game_lines(&phase);
        Paragraph::new(content_lines).render(game_inner, frame.buffer_mut());

        if ctx.show_typestate_graph && main_chunks.len() > 1 {
            TypestateGraphWidget::new(ctx.bj_nodes, ctx.bj_edges, active, event_log)
                .with_context(&phase_ctx)
                .render(main_chunks[1], frame.buffer_mut());
        }

        // Prompt pane — bordered region for TuiCommunicator input.
        let prompt_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(prompt_block, prompt_area);
    })?;
    Ok(())
}

#[instrument(skip_all)]
fn render_finish<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    finished: &GameFinished,
    outcome: &BlackjackSessionOutcome,
    event_log: &[GameEvent],
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    render_blackjack(
        ctx,
        DisplayPhase::Finished { state: finished },
        blackjack_active("Finished"),
        event_log,
    )?;

    ctx.terminal.draw(|frame| {
        let area = frame.area();
        let outcome_text = match outcome {
            BlackjackSessionOutcome::Win(b) => format!("🎉  You WIN! Bankroll: {b}"),
            BlackjackSessionOutcome::Loss(b) => format!("💸  You lose. Bankroll: {b}"),
            BlackjackSessionOutcome::Push(b) => format!("🤝  Push. Bankroll: {b}"),
            BlackjackSessionOutcome::Abandoned => "Session ended.".to_string(),
        };
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                &outcome_text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  — press any key to continue"),
        ]));
        let h = 3u16;
        let w = (area.width as usize).min(outcome_text.len() + 40) as u16;
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let rect = ratatui::layout::Rect {
            x,
            y,
            width: w,
            height: h,
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
        let inner = block.inner(rect);
        block.render(rect, frame.buffer_mut());
        footer.render(inner, frame.buffer_mut());
    })?;
    Ok(())
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

fn build_game_lines(phase: &DisplayPhase<'_>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match phase {
        DisplayPhase::Betting { state } => {
            lines.push(Line::from(Span::styled(
                "♦  Place your bet",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "  Bankroll: {} chips",
                state.bankroll()
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  (Enter amount below ↓)",
                Style::default().fg(Color::DarkGray),
            )));
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
            lines.push(Line::from(dealer_str));
            lines.push(Line::from(""));

            for (i, hand) in state.player_hands().iter().enumerate() {
                let marker = if i == state.current_hand_index() {
                    "▶"
                } else {
                    " "
                };
                lines.push(Line::from(Span::styled(
                    format!("  {marker} Your hand: {hand}"),
                    if i == state.current_hand_index() {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  (Choose action below ↓)",
                Style::default().fg(Color::DarkGray),
            )));
        }
        DisplayPhase::Finished { state } => {
            let dealer_hand = state.dealer_hand();
            let dealer_natural = dealer_hand.is_blackjack();
            let player_natural = state
                .player_hands()
                .first()
                .is_some_and(|h| h.is_blackjack());

            // ── Context banner ─────────────────────────────────
            let (banner, banner_color) = match (player_natural, dealer_natural) {
                (true, true) => ("  ♦ Both have natural blackjack — Push!", Color::Yellow),
                (true, false) => ("  ♠ Natural blackjack! 3:2 payout", Color::Green),
                (false, true) => ("  ♦ Dealer natural blackjack", Color::Red),
                (false, false) => ("", Color::White),
            };
            if !banner.is_empty() {
                lines.push(Line::from(Span::styled(
                    banner,
                    Style::default()
                        .fg(banner_color)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
            }

            lines.push(Line::from(format!("  Dealer: {dealer_hand}")));
            lines.push(Line::from(""));

            for (i, (hand, outcome)) in state
                .player_hands()
                .iter()
                .zip(state.outcomes().iter())
                .enumerate()
            {
                let bet = state.bets().get(i).copied().unwrap_or(0);
                let is_bust = hand.is_bust();
                let color = if outcome.is_win() {
                    Color::Green
                } else if outcome.is_loss() {
                    Color::Red
                } else {
                    Color::Yellow
                };
                let bust_note = if is_bust { "  BUST" } else { "" };
                lines.push(Line::from(Span::styled(
                    format!(
                        "  Hand {}: {}  [{outcome}]{bust_note}  (bet: {bet})",
                        i + 1,
                        hand
                    ),
                    Style::default().fg(color),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "  Final bankroll: {} chips",
                state.bankroll()
            )));
        }
    }

    lines
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

/// Runs a multi-player blackjack session with one human and zero or more AI agents.
///
/// All players compete independently against the house from a single shared deck.
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
    use strictly_blackjack::MultiRound;

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
        render_multi(
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
        let mut round = MultiRound::deal(seat_bets).map_err(anyhow::Error::msg)?;

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

                    render_multi(
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
                    )?;

                    if round.seats[round_idx].is_done() {
                        break;
                    }

                    let action_opt = match &seat_comms[bankroll_idx] {
                        SeatComm::Human { comm, .. } => elicit_action_from(comm).await,
                        SeatComm::Agent { comm, .. } => elicit_action_from(comm).await,
                    };
                    let Some(action) = action_opt else {
                        break 'session;
                    };

                    match action {
                        BasicAction::Hit => {
                            let deck = &mut round.deck;
                            round.seats[round_idx]
                                .hit(deck)
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

        render_multi(
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
fn render_multi<B: Backend>(terminal: &mut Terminal<B>, ctx: MultiRenderCtx<'_>) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::tui::ChatWidget;
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

    terminal.draw(|frame| {
        let full = frame.area();

        // Split: game content on top, dedicated prompt pane at bottom.
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(PROMPT_PANE_HEIGHT)])
            .split(full);
        let area = outer[0];
        let prompt_area = outer[1];

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(35),
                Constraint::Percentage(25),
            ])
            .split(area);

        // ── Left: all players' hands ─────────────────────────────────────
        // Split left pane vertically: dealer (fixed 4 lines) + seats (remaining).
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(chunks[0]);

        // Dealer hand (fixed section at top).
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
        let dealer_widget =
            Paragraph::new(vec![Line::from(format!("  {dealer_str}{dealer_value}"))])
                .block(Block::default().borders(Borders::ALL).title(" Dealer "));
        frame.render_widget(dealer_widget, left_chunks[0]);

        // Player seats (scrollable).
        let mut hand_lines: Vec<Line<'static>> = Vec::new();
        for (i, seat) in seats.iter().enumerate() {
            let color = if i == active_seat {
                Color::Green
            } else {
                Color::White
            };
            let prefix = if i == active_seat { "► " } else { "  " };

            hand_lines.push(Line::from(Span::styled(
                format!("{prefix}{} (bet {})  ", seat.name, seat.bet),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )));
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
            hand_lines.push(Line::from(format!("    {cards_str}  {status}")));
        }

        // When no seats exist yet (betting phase), show bankrolls.
        if seats.is_empty() {
            for (i, name) in seat_names.iter().enumerate() {
                let bankroll = bankrolls.get(i).copied().unwrap_or(0);
                hand_lines.push(Line::from(format!("  {name}: {bankroll} chips")));
            }
        }

        // Scroll to keep active seat visible (2 lines per seat).
        let seats_height = left_chunks[1].height.saturating_sub(2) as usize;
        let scroll_offset = if active_seat < seats.len() {
            let seat_line = active_seat * 2;
            seat_line.saturating_sub(seats_height / 2)
        } else if hand_lines.len() > seats_height {
            hand_lines.len().saturating_sub(seats_height)
        } else {
            0
        };

        let seats_widget = Paragraph::new(hand_lines)
            .block(Block::default().borders(Borders::ALL).title(" Seats "))
            .scroll((scroll_offset as u16, 0));
        frame.render_widget(seats_widget, left_chunks[1]);

        // ── Center: typestate + log ───────────────────────────────────────
        if show_typestate_graph {
            let phase_ctx =
                PhaseContext::info("Playing…").with_pending_prompt(prompt.map(|s| s.to_string()));
            let widget = TypestateGraphWidget::new(
                bj_nodes,
                bj_edges,
                blackjack_active("PlayerTurn"),
                event_log,
            )
            .with_context(&phase_ctx);
            frame.render_widget(widget, chunks[1]);
        } else {
            let log_lines: Vec<Line<'static>> = event_log
                .iter()
                .rev()
                .take(chunks[1].height.saturating_sub(2) as usize)
                .map(|e| Line::from(e.text.clone()))
                .collect();
            let log = Paragraph::new(log_lines)
                .block(Block::default().borders(Borders::ALL).title(" Game Log "));
            frame.render_widget(log, chunks[1]);
        }

        // ── Right: chat log ───────────────────────────────────────────────
        let chat = ChatWidget::new(chat_messages);
        frame.render_widget(chat, chunks[2]);

        // ── Bottom: dedicated prompt pane for TuiCommunicator input ──────
        let prompt_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(prompt_block, prompt_area);
    })?;
    Ok(())
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
