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

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
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
    PlaceBetOutput, PlayActionOutput, PlayActionResult, execute_dealer_turn, execute_place_bet,
    execute_play_action,
};
use tokio::time::{Duration, sleep};
use tracing::{info, instrument, warn};
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    ChoiceHint, GameEvent, PhaseContext, TypestateGraphWidget, blackjack_active, blackjack_edges,
    blackjack_nodes,
};
use elicitation::Elicitation as _;

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

    let comm = TuiCommunicator::new();
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
    comm: &TuiCommunicator,
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
        let raw: u64 = match u64::elicit(comm).await {
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
    comm: &TuiCommunicator,
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
    let phase_ctx = build_phase_context(&phase);

    ctx.terminal.draw(|frame| {
        let area = frame.area();

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

async fn wait_for_keypress() -> Result<bool> {
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
            // Q anywhere is a passive escape.
            let quit = matches!(k.code, KeyCode::Char('q') | KeyCode::Char('Q'));
            return Ok(!quit);
        }
    }
}
