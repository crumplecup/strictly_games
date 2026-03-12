//! Blackjack TUI game loop.
//!
//! Drives a local blackjack game session entirely within the ratatui terminal.
//! The [`TuiCommunicator`] is used for all player decisions so that the same
//! elicitation interface works for both human players (in the TUI) and future
//! AI agent passengers riding along through the same interface.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    prelude::Widget,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use strictly_blackjack::Outcome;use tokio::time::{Duration, sleep};
use tracing::{info, instrument, warn};

use crate::games::blackjack::{
    BasicAction, GameBetting, GameFinished, GamePlayerTurn, GameResult, GameSetup, PlayerAction,
};
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    GameEvent, TypestateGraphWidget, blackjack_active, blackjack_edges, blackjack_nodes,
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

// ─────────────────────────────────────────────────────────────
//  Public entry point
// ─────────────────────────────────────────────────────────────

/// Run a complete blackjack session in the TUI.
///
/// The game is entirely local (no REST server). Player decisions are elicited
/// via [`TuiCommunicator`]. Returns the session outcome so the caller can
/// record stats and return to the lobby.
///
/// # Arguments
///
/// * `terminal` — ratatui terminal (must already be in raw mode)
/// * `player_name` — display name shown in the UI
/// * `initial_bankroll` — starting chip count (default: 1000)
/// * `show_typestate_graph` — whether to render the phase diagram panel
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
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut current_phase = "Betting".to_string();

    // Initialise game: Setup → Betting.
    let game = GameSetup::new();
    let betting = game.start_betting(initial_bankroll);

    event_log.push(GameEvent::phase_change("Setup", "Betting"));

    // ── Betting phase ────────────────────────────────────────────
    let active_node = blackjack_active(&current_phase);
    render_blackjack(
        terminal,
        DisplayPhase::Betting { state: &betting },
        &player_name,
        show_typestate_graph,
        &bj_nodes,
        &bj_edges,
        active_node,
        &event_log,
    )?;

    // Elicit bet via TuiCommunicator — same interface for human and agent.
    let bet = loop {
        let raw: u64 = match u64::elicit(&comm).await {
            Ok(v) => v,
            Err(_) => {
                warn!("Elicitation cancelled during betting");
                return Ok(BlackjackSessionOutcome::Abandoned);
            }
        };
        if raw == 0 || raw > betting.bankroll() {
            // re-elicit; TuiCommunicator already printed the rejection info
            continue;
        }
        break raw;
    };

    // Transition Betting → PlayerTurn (or Finished on instant blackjack).
    let result = betting.place_bet(bet)?;

    let outcome = match result {
        // Instant blackjack or dealer natural — go straight to finished display.
        GameResult::Finished(ref finished) => {
            event_log.push(GameEvent::phase_change("Betting", "Finished"));
            let outcome = compute_outcome(finished);
            event_log.push(GameEvent::result(&format!("{outcome:?}")));
            render_finish(
                terminal,
                finished,
                &player_name,
                &outcome,
                show_typestate_graph,
                &bj_nodes,
                &bj_edges,
                &event_log,
            )?;
            wait_for_keypress().await?;
            return Ok(outcome);
        }
        // Normal game — proceed to player turn.
        GameResult::PlayerTurn(state) => {
            current_phase = "PlayerTurn".to_string();
            event_log.push(GameEvent::phase_change("Betting", "PlayerTurn"));
            event_log.push(GameEvent::proof("BetPlaced"));
            play_player_turn(
                terminal,
                state,
                &comm,
                &player_name,
                show_typestate_graph,
                &bj_nodes,
                &bj_edges,
                &mut current_phase,
                &mut event_log,
            )
            .await?
        }
        GameResult::DealerTurn(state) => {
            // Unusual path — proceed to dealer turn directly.
            event_log.push(GameEvent::phase_change("Betting", "DealerTurn"));
            state.play_dealer_turn()
        }
    };

    // ── Show result ──────────────────────────────────────────────
    let session_outcome = compute_outcome(&outcome);
    event_log.push(GameEvent::result(&format!("{session_outcome:?}")));
    render_finish(
        terminal,
        &outcome,
        &player_name,
        &session_outcome,
        show_typestate_graph,
        &bj_nodes,
        &bj_edges,
        &event_log,
    )?;
    wait_for_keypress().await?;

    Ok(session_outcome)
}

// ─────────────────────────────────────────────────────────────
//  Player turn loop
// ─────────────────────────────────────────────────────────────

/// Handles the player turn phase, returning the finished game.
#[instrument(skip_all)]
async fn play_player_turn<B: Backend>(
    terminal: &mut Terminal<B>,
    mut state: GamePlayerTurn,
    comm: &TuiCommunicator,
    player_name: &str,
    show_typestate_graph: bool,
    bj_nodes: &[crate::tui::typestate_widget::NodeDef],
    bj_edges: &[crate::tui::typestate_widget::EdgeDef],
    current_phase: &mut String,
    event_log: &mut Vec<GameEvent>,
) -> Result<GameFinished>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        render_blackjack(
            terminal,
            DisplayPhase::PlayerTurn { state: &state },
            player_name,
            show_typestate_graph,
            bj_nodes,
            bj_edges,
            blackjack_active(current_phase),
            event_log,
        )?;

        let action = match BasicAction::elicit(comm).await {
            Ok(a) => a,
            Err(_) => {
                warn!("Elicitation cancelled during player turn");
                // Stand on cancel to reach a terminal state cleanly.
                BasicAction::Stand
            }
        };

        let player_action = PlayerAction::new(action, state.current_hand_index());
        event_log.push(GameEvent::proof("LegalMove"));

        match state.clone().take_action(player_action) {
            Ok(GameResult::PlayerTurn(next)) => {
                state = next;
            }
            Ok(GameResult::DealerTurn(dealer)) => {
                *current_phase = "DealerTurn".to_string();
                event_log.push(GameEvent::phase_change("PlayerTurn", "DealerTurn"));
                let finished = dealer.play_dealer_turn();
                *current_phase = "Finished".to_string();
                event_log.push(GameEvent::phase_change("DealerTurn", "Finished"));
                return Ok(finished);
            }
            Ok(GameResult::Finished(finished)) => {
                *current_phase = "Finished".to_string();
                event_log.push(GameEvent::phase_change("PlayerTurn", "Finished"));
                return Ok(finished);
            }
            Err(e) => {
                warn!(error = %e, "Invalid action — re-prompting");
                event_log.push(GameEvent::result(&format!("Invalid: {e}")));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Outcome helpers
// ─────────────────────────────────────────────────────────────

/// Derives the session outcome from the finished game state.
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

/// Renders the current blackjack game state into the ratatui terminal.
#[instrument(skip_all)]
fn render_blackjack<B: Backend>(
    terminal: &mut Terminal<B>,
    phase: DisplayPhase<'_>,
    player_name: &str,
    show_typestate_graph: bool,
    bj_nodes: &[crate::tui::typestate_widget::NodeDef],
    bj_edges: &[crate::tui::typestate_widget::EdgeDef],
    active: Option<usize>,
    event_log: &[GameEvent],
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    terminal.draw(|frame| {
        let area = frame.area();

        let main_chunks = if show_typestate_graph {
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

        // ── Game panel ──────────────────────────────────────────
        let game_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" ♠ Blackjack — {} ♣ ", player_name))
            .style(Style::default().fg(Color::White));

        let game_inner = game_block.inner(main_chunks[0]);
        game_block.render(main_chunks[0], frame.buffer_mut());

        let content_lines = build_game_lines(&phase);
        Paragraph::new(content_lines).render(game_inner, frame.buffer_mut());

        // ── Typestate panel ─────────────────────────────────────
        if show_typestate_graph && main_chunks.len() > 1 {
            TypestateGraphWidget::new(bj_nodes, bj_edges, active, event_log)
                .render(main_chunks[1], frame.buffer_mut());
        }
    })?;
    Ok(())
}

/// Renders the game-over screen and waits for any keypress.
#[instrument(skip_all)]
fn render_finish<B: Backend>(
    terminal: &mut Terminal<B>,
    finished: &GameFinished,
    player_name: &str,
    outcome: &BlackjackSessionOutcome,
    show_typestate_graph: bool,
    bj_nodes: &[crate::tui::typestate_widget::NodeDef],
    bj_edges: &[crate::tui::typestate_widget::EdgeDef],
    event_log: &[GameEvent],
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    render_blackjack(
        terminal,
        DisplayPhase::Finished { state: finished },
        player_name,
        show_typestate_graph,
        bj_nodes,
        bj_edges,
        blackjack_active("Finished"),
        event_log,
    )?;

    // Show a result overlay.
    terminal.draw(|frame| {
        let area = frame.area();
        let outcome_text = match outcome {
            BlackjackSessionOutcome::Win(b)  => format!("🎉  You WIN! Bankroll: {b}"),
            BlackjackSessionOutcome::Loss(b) => format!("💸  You lose. Bankroll: {b}"),
            BlackjackSessionOutcome::Push(b) => format!("🤝  Push. Bankroll: {b}"),
            BlackjackSessionOutcome::Abandoned => "Session ended.".to_string(),
        };
        let footer = Paragraph::new(Line::from(vec![
            Span::styled(&outcome_text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("  — press any key to continue"),
        ]));
        let h = 3u16;
        let w = (area.width as usize).min(outcome_text.len() + 40) as u16;
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let rect = ratatui::layout::Rect { x, y, width: w, height: h };
        let block = Block::default().borders(Borders::ALL).style(
            Style::default().bg(Color::DarkGray).fg(Color::White)
        );
        let inner = block.inner(rect);
        block.render(rect, frame.buffer_mut());
        footer.render(inner, frame.buffer_mut());
    })?;
    Ok(())
}

/// Builds the ratatui [`Line`]s for the game panel.
fn build_game_lines(phase: &DisplayPhase<'_>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match phase {
        DisplayPhase::Betting { state } => {
            lines.push(Line::from(Span::styled(
                "♦  Place your bet",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
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
                let visible = dealer_cards.first().map(|c| c.to_string()).unwrap_or_default();
                format!("  Dealer: {visible} [?]  (hit 17+)")
            };
            lines.push(Line::from(dealer_str));
            lines.push(Line::from(""));

            for (i, hand) in state.player_hands().iter().enumerate() {
                let marker = if i == state.current_hand_index() { "▶" } else { " " };
                lines.push(Line::from(Span::styled(
                    format!("  {marker} Your hand: {hand}"),
                    if i == state.current_hand_index() {
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
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
            lines.push(Line::from(format!("  Dealer: {dealer_hand}")));
            lines.push(Line::from(""));

            for (i, (hand, outcome)) in state
                .player_hands()
                .iter()
                .zip(state.outcomes().iter())
                .enumerate()
            {
                let bet = state.bets().get(i).copied().unwrap_or(0);
                let color = if outcome.is_win() {
                    Color::Green
                } else if outcome.is_loss() {
                    Color::Red
                } else {
                    Color::Yellow
                };
                lines.push(Line::from(Span::styled(
                    format!("  Hand {}: {}  [{outcome}]  (bet: {bet})", i + 1, hand),
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

/// Blocks until the player presses any key (used on game-over screen).
async fn wait_for_keypress() -> Result<()> {
    loop {
        sleep(Duration::from_millis(50)).await;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                if k.code != KeyCode::Null {
                    return Ok(());
                }
            }
        }
    }
}
