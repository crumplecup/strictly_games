//! Craps TUI game loop.
//!
//! Drives a local craps trainer session entirely within the ratatui terminal.
//! The [`TuiCommunicator`] handles all player decisions via the elicitation
//! framework, so the same interface works for humans and AI agents.
//!
//! Game logic is wired through proof-carrying workflow tools:
//! `execute_place_bets` → `execute_comeout_roll` → `execute_point_roll`.
//! The compiler enforces correct phase ordering via `Established<P>` contracts.

use crate::tui::observable_communicator::ObservableCommunicator;
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    GameEvent, PhaseContext, TypestateGraphWidget, craps_active, craps_edges, craps_nodes,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use elicitation::ElicitCommunicator as _;
use elicitation::Elicitation as _;
use rand::SeedableRng as _;
use rand::rngs::SmallRng;
use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Direction, Layout},
    prelude::Widget,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use strictly_craps::{
    ActiveBet, BetOutcome, BetType, ComeOutOutput, CrapsTable, DiceRoll, GameSetup, Point,
    PointRollOutput, execute_comeout_roll, execute_place_bets, execute_point_roll,
};
use tokio::sync::watch;
use tracing::{info, instrument, warn};

/// Height of the dedicated prompt pane at the bottom.
pub const PROMPT_PANE_HEIGHT: u16 = 10;

/// Outcome of a complete craps session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrapsSessionOutcome {
    /// Player walked away with chips.
    CashedOut(u64),
    /// Player lost all chips.
    Busted,
    /// Player abandoned the session.
    Abandoned,
}

/// Which phase to display in the TUI.
#[derive(Debug)]
enum DisplayPhase<'a> {
    Betting {
        bankroll: u64,
        lesson_title: &'a str,
        lesson_level: u8,
        lesson_tip: &'a str,
    },
    ComeOut {
        bankroll: u64,
        bets: &'a [ActiveBet],
        roll: Option<DiceRoll>,
    },
    PointPhase {
        bankroll: u64,
        bets: &'a [ActiveBet],
        point: Point,
        roll: Option<DiceRoll>,
    },
    Resolved {
        bankroll: u64,
        results: &'a [(ActiveBet, BetOutcome)],
        pass_line_won: bool,
        point: Option<Point>,
    },
}

/// Shared rendering context threaded through all render calls.
struct RenderCtx<'a, B: Backend> {
    terminal: &'a mut Terminal<B>,
    player_name: &'a str,
    show_typestate_graph: bool,
    nodes: &'a [crate::tui::typestate_widget::NodeDef],
    edges: &'a [crate::tui::typestate_widget::EdgeDef],
    prompt_rx: watch::Receiver<Option<String>>,
}

// ─────────────────────────────────────────────────────────────
//  Public entry point
// ─────────────────────────────────────────────────────────────

/// Run a complete craps trainer session in the TUI.
///
/// Manages rounds of craps with progressive lesson unlocks. The dealer is
/// fully automated; dice rolls use random generation. Returns the session
/// outcome for lobby stats.
#[instrument(skip_all, fields(player_name = %player_name, initial_bankroll, show_typestate_graph))]
pub async fn run_craps_session<B: Backend>(
    terminal: &mut Terminal<B>,
    player_name: String,
    initial_bankroll: u64,
    show_typestate_graph: bool,
) -> Result<CrapsSessionOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    info!("Starting craps session");

    let (prompt_tx, prompt_rx) = watch::channel(None::<String>);
    let comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx);
    let nodes = craps_nodes();
    let edges = craps_edges();
    let mut rng = SmallRng::from_entropy();
    let mut event_log: Vec<GameEvent> = Vec::new();

    // Initialize table with single human player
    let mut table = CrapsTable::new(3, 5, 500);
    let seat =
        strictly_craps::CrapsSeat::new(player_name.clone(), initial_bankroll).with_shooter(true);
    table.add_seat(seat);

    event_log.push(GameEvent::story(format!(
        "🎲  Welcome to Craps! Bankroll: ${initial_bankroll}"
    )));
    event_log.push(GameEvent::story(format!(
        "📖  Lesson 1: {}",
        table.seats()[0].lesson().lesson_title()
    )));

    loop {
        let bankroll = *table.seats()[0].bankroll();
        if bankroll == 0 {
            info!("Bankroll exhausted — ending session");
            return Ok(CrapsSessionOutcome::Busted);
        }

        let mut ctx = RenderCtx {
            terminal,
            player_name: &player_name,
            show_typestate_graph,
            nodes: &nodes,
            edges: &edges,
            prompt_rx: prompt_rx.clone(),
        };

        let outcome =
            run_single_round(&mut ctx, &mut table, &comm, &mut event_log, &mut rng).await?;

        if matches!(outcome, RoundOutcome::Quit) {
            let final_bankroll = *table.seats()[0].bankroll();
            return Ok(if final_bankroll > 0 {
                CrapsSessionOutcome::CashedOut(final_bankroll)
            } else {
                CrapsSessionOutcome::Abandoned
            });
        }

        // Check lesson advancement
        if table.seats()[0].lesson().can_advance() {
            let seat = &mut table.seat_mut(0).expect("seat 0 exists");
            if seat.advance_round() {
                let new_title = seat.lesson().lesson_title();
                let new_level = seat.lesson().level();
                event_log.push(GameEvent::story(format!(
                    "🎓  Level up! Lesson {new_level}: {new_title}"
                )));
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Single round
// ─────────────────────────────────────────────────────────────

/// What to do after a round completes.
enum RoundOutcome {
    /// Continue to next round.
    Continue,
    /// Player quit (pressed Q or cancelled).
    Quit,
}

/// Runs one round. Returns whether to continue or quit.
#[instrument(skip_all)]
async fn run_single_round<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    table: &mut CrapsTable,
    comm: &ObservableCommunicator<TuiCommunicator>,
    event_log: &mut Vec<GameEvent>,
    rng: &mut SmallRng,
) -> Result<RoundOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let bankroll = *table.seats()[0].bankroll();
    let lesson = table.seats()[0].lesson();
    let mut current_phase = "Betting".to_string();

    event_log.push(GameEvent::story(format!(
        "🂠  New round — bankroll: ${bankroll}"
    )));

    // Show lesson progress hint
    let remaining = lesson
        .advance_threshold()
        .saturating_sub(lesson.rounds_at_level());
    if remaining < u32::MAX && remaining > 0 {
        event_log.push(GameEvent::story(format!(
            "   📚 Lesson {}: {} rounds until next unlock",
            lesson.level(),
            remaining
        )));
    }

    // ── Betting phase ─────────────────────────────────────────
    render_craps(
        ctx,
        DisplayPhase::Betting {
            bankroll,
            lesson_title: lesson.lesson_title(),
            lesson_level: lesson.level(),
            lesson_tip: lesson.lesson_text(),
        },
        craps_active(&current_phase),
        event_log,
    )?;

    let bet_amount = loop {
        let max_bet = bankroll.min(table.table_max());
        let styled = comm.with_style::<u64, CrapsBetStyle>(CrapsBetStyle::new(
            table.table_min(),
            max_bet,
            bankroll,
        ));
        let raw: u64 = match u64::elicit(&styled).await {
            Ok(v) => v,
            Err(_) => {
                warn!("Elicitation cancelled during betting");
                return Ok(RoundOutcome::Quit);
            }
        };
        if raw < table.table_min() || raw > max_bet {
            continue;
        }
        break raw;
    };

    // Deduct from bankroll
    table
        .seat_mut(0)
        .expect("seat 0 exists")
        .deduct_wagers(bet_amount)
        .map_err(anyhow::Error::msg)?;

    let bet = ActiveBet::new(BetType::PassLine, bet_amount);
    let seat_bets = vec![vec![bet.clone()]];

    event_log.push(GameEvent::story(format!(
        "💰  Pass Line ${bet_amount} — looking for 7 or 11"
    )));
    event_log.push(GameEvent::proof("BetsPlaced"));

    // ── Setup typestate and execute_place_bets ─────────────────
    let setup = GameSetup::new(1, table.max_odds());
    let betting_state = setup.start_betting(table.bankroll_vec());
    let (comeout_state, bets_proof) =
        execute_place_bets(betting_state, seat_bets.clone()).map_err(anyhow::Error::msg)?;

    current_phase = "ComeOut".to_string();
    event_log.push(GameEvent::phase_change("Betting", "ComeOut"));

    // ── Come-out roll ─────────────────────────────────────────
    let roll = DiceRoll::random(rng);
    let sum = roll.sum();

    render_craps(
        ctx,
        DisplayPhase::ComeOut {
            bankroll: *table.seats()[0].bankroll(),
            bets: &seat_bets[0],
            roll: Some(roll),
        },
        craps_active(&current_phase),
        event_log,
    )?;

    event_log.push(GameEvent::story(format!(
        "🎲  Come-out roll: {} + {} = {sum}",
        roll.die1(),
        roll.die2(),
    )));

    match execute_comeout_roll(comeout_state, roll, bets_proof) {
        ComeOutOutput::Resolved(resolved, _settled_proof) => {
            current_phase = "Resolved".to_string();
            let pass_won = resolved.pass_line_won();
            let results = table.settle_round(&seat_bets, resolved.final_roll(), None, true);

            if pass_won {
                // Credit back wager + winnings
                table
                    .seat_mut(0)
                    .expect("seat 0")
                    .credit_winnings(bet_amount * 2);
                let natural_word = if sum == 7 { "Seven" } else { "Yo-leven" };
                event_log.push(GameEvent::story(format!(
                    "✨  Natural {natural_word}! — Pass Line wins ${bet_amount} (1:1)"
                )));
                event_log.push(GameEvent::story(
                    "   7 and 11 are automatic Pass Line winners on come-out".to_string(),
                ));
            } else if roll.is_craps() {
                let (craps_word, tip) = match sum {
                    2 => ("Snake Eyes", "two aces — only one way to roll it"),
                    3 => ("Ace-Deuce", "rarest craps roll after Snake Eyes"),
                    12 => ("Boxcars", "double sixes — Don't Pass pushes (bar 12)"),
                    _ => ("Craps", ""),
                };
                event_log.push(GameEvent::story(format!(
                    "💀  {craps_word} ({sum})! — Pass Line loses ${bet_amount}"
                )));
                if !tip.is_empty() {
                    event_log.push(GameEvent::story(format!("   {tip}")));
                }
            }

            // Narrate individual bet outcomes
            let seat_results = if !results.is_empty() {
                let outcomes: Vec<_> = results[0]
                    .outcomes()
                    .iter()
                    .map(|(b, o)| (b.clone(), *o))
                    .collect();
                for (bet, outcome) in &outcomes {
                    event_log.push(GameEvent::story(narrate_outcome(bet, outcome)));
                }
                outcomes
            } else {
                vec![]
            };

            event_log.push(GameEvent::phase_change("ComeOut", "Resolved"));
            event_log.push(GameEvent::proof("RoundSettled"));

            render_craps(
                ctx,
                DisplayPhase::Resolved {
                    bankroll: *table.seats()[0].bankroll(),
                    results: &seat_results,
                    pass_line_won: pass_won,
                    point: None,
                },
                craps_active(&current_phase),
                event_log,
            )?;

            table.seat_mut(0).expect("seat 0").record_round();

            let new_bankroll = *table.seats()[0].bankroll();
            let net: i64 = if pass_won {
                bet_amount as i64
            } else {
                -(bet_amount as i64)
            };
            let sign = if net >= 0 { "+" } else { "" };
            event_log.push(GameEvent::result(format!(
                "Round over: {sign}${} · bankroll: ${new_bankroll}",
                net.unsigned_abs()
            )));

            return wait_for_continue(ctx, event_log).await;
        }
        ComeOutOutput::PointSet(mut point_phase, point_proof) => {
            let point = point_phase.point();
            current_phase = "PointPhase".to_string();
            let ways_point = ways_to_roll(point.value());
            event_log.push(GameEvent::story(format!(
                "📍  Point is {point}! Puck ON — need {point} before 7"
            )));
            event_log.push(GameEvent::story(format!(
                "   {ways_point} ways to hit {point} vs 6 ways to roll 7"
            )));
            event_log.push(GameEvent::phase_change("ComeOut", "PointPhase"));
            event_log.push(GameEvent::proof("PointEstablished"));

            // ── Point phase rolls ─────────────────────────────
            let mut current_proof = point_proof;
            let mut roll_count: u32 = 0;
            loop {
                render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: None,
                    },
                    craps_active(&current_phase),
                    event_log,
                )?;

                // Wait for keypress to roll
                event_log.push(GameEvent::story("  Press any key to roll...".to_string()));
                render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: None,
                    },
                    craps_active(&current_phase),
                    event_log,
                )?;

                if matches!(wait_for_keypress_raw().await?, RoundOutcome::Quit) {
                    return Ok(RoundOutcome::Quit);
                }

                roll_count += 1;
                let point_roll = DiceRoll::random(rng);
                let point_sum = point_roll.sum();
                event_log.push(GameEvent::story(format!(
                    "🎲  Roll #{roll_count}: {} + {} = {point_sum}",
                    point_roll.die1(),
                    point_roll.die2(),
                )));

                render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: Some(point_roll),
                    },
                    craps_active(&current_phase),
                    event_log,
                )?;

                match execute_point_roll(point_phase, point_roll, current_proof) {
                    PointRollOutput::Continue(next, proof) => {
                        event_log.push(GameEvent::story(format!(
                            "   {point_sum} — no decision after {roll_count} rolls \
                             (need {point} or 7)"
                        )));
                        point_phase = next;
                        current_proof = proof;
                    }
                    PointRollOutput::Resolved(resolved, _settled_proof) => {
                        current_phase = "Resolved".to_string();
                        let pass_won = resolved.pass_line_won();

                        let results = table.settle_round(
                            &seat_bets,
                            resolved.final_roll(),
                            Some(point),
                            false,
                        );

                        if pass_won {
                            table
                                .seat_mut(0)
                                .expect("seat 0")
                                .credit_winnings(bet_amount * 2);
                            event_log.push(GameEvent::story(format!(
                                "🎯  Point {point} hit on roll #{roll_count}! \
                                 Pass Line wins ${bet_amount} (1:1)"
                            )));
                        } else {
                            event_log.push(GameEvent::story(format!(
                                "💀  Seven-out on roll #{roll_count}! \
                                 Pass Line loses ${bet_amount}"
                            )));
                            event_log.push(GameEvent::story(
                                "   7 has 6 ways — the most of any number".to_string(),
                            ));
                        }

                        // Narrate individual bet outcomes
                        let seat_results = if !results.is_empty() {
                            let outcomes: Vec<_> = results[0]
                                .outcomes()
                                .iter()
                                .map(|(b, o)| (b.clone(), *o))
                                .collect();
                            for (bet, outcome) in &outcomes {
                                event_log.push(GameEvent::story(narrate_outcome(bet, outcome)));
                            }
                            outcomes
                        } else {
                            vec![]
                        };

                        event_log.push(GameEvent::phase_change("PointPhase", "Resolved"));
                        event_log.push(GameEvent::proof("RoundSettled"));

                        render_craps(
                            ctx,
                            DisplayPhase::Resolved {
                                bankroll: *table.seats()[0].bankroll(),
                                results: &seat_results,
                                pass_line_won: pass_won,
                                point: Some(point),
                            },
                            craps_active(&current_phase),
                            event_log,
                        )?;

                        table.seat_mut(0).expect("seat 0").record_round();

                        let new_bankroll = *table.seats()[0].bankroll();
                        let net: i64 = if pass_won {
                            bet_amount as i64
                        } else {
                            -(bet_amount as i64)
                        };
                        let sign = if net >= 0 { "+" } else { "" };
                        event_log.push(GameEvent::result(format!(
                            "Round over: {sign}${} · bankroll: ${new_bankroll}",
                            net.unsigned_abs()
                        )));

                        return wait_for_continue(ctx, event_log).await;
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Rendering
// ─────────────────────────────────────────────────────────────

#[instrument(skip_all)]
fn render_craps<B: Backend>(
    ctx: &mut RenderCtx<'_, B>,
    phase: DisplayPhase<'_>,
    active: Option<usize>,
    event_log: &[GameEvent],
) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    let pending_prompt = ctx.prompt_rx.borrow().clone();
    let phase_ctx = build_phase_context(&phase).with_pending_prompt(pending_prompt);

    ctx.terminal.draw(|frame| {
        let full = frame.area();

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(PROMPT_PANE_HEIGHT)])
            .split(full);
        let area = outer[0];
        let prompt_area = outer[1];

        // 3-column: Game (40%) | Story (35%) | Typestate (25%)
        // Without typestate graph: Game (45%) | Story (55%)
        let main_chunks = if ctx.show_typestate_graph {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(40),
                    Constraint::Percentage(35),
                    Constraint::Percentage(25),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
                .split(area)
        };

        // ── Left: Game state ──────────────────────────────────
        let game_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" 🎲 Craps — {} 🎲 ", ctx.player_name))
            .style(Style::default().fg(Color::White));

        let game_inner = game_block.inner(main_chunks[0]);
        game_block.render(main_chunks[0], frame.buffer_mut());

        let content_lines = build_game_lines(&phase);
        Paragraph::new(content_lines).render(game_inner, frame.buffer_mut());

        // ── Center: Story log ─────────────────────────────────
        let story_idx = 1;
        let story_block = Block::default()
            .borders(Borders::ALL)
            .title(" Story ")
            .style(Style::default().fg(Color::White));
        let story_inner = story_block.inner(main_chunks[story_idx]);
        story_block.render(main_chunks[story_idx], frame.buffer_mut());

        let story_height = story_inner.height as usize;
        let story_lines: Vec<Line<'_>> = event_log
            .iter()
            .map(|ev| Line::from(Span::styled(ev.text.clone(), Style::default().fg(ev.color))))
            .collect();
        // Show most recent events (scroll to bottom)
        let skip = story_lines.len().saturating_sub(story_height);
        let visible: Vec<Line<'_>> = story_lines.into_iter().skip(skip).collect();
        Paragraph::new(visible)
            .wrap(Wrap { trim: false })
            .render(story_inner, frame.buffer_mut());

        // ── Right: Typestate graph (if enabled) ───────────────
        if ctx.show_typestate_graph && main_chunks.len() > 2 {
            TypestateGraphWidget::new(ctx.nodes, ctx.edges, active, event_log)
                .with_context(&phase_ctx)
                .render(main_chunks[2], frame.buffer_mut());
        }

        let prompt_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(prompt_block, prompt_area);
    })?;
    Ok(())
}

fn build_phase_context(phase: &DisplayPhase<'_>) -> PhaseContext {
    match phase {
        DisplayPhase::Betting {
            bankroll,
            lesson_title,
            lesson_level,
            ..
        } => PhaseContext::info(format!(
            "Lesson {lesson_level}: {lesson_title} — bankroll: ${bankroll}"
        )),

        DisplayPhase::ComeOut { roll, .. } => {
            if let Some(r) = roll {
                PhaseContext::info(format!("Come-out roll: {} = {}", r, r.sum()))
            } else {
                PhaseContext::info("Awaiting come-out roll...".to_string())
            }
        }

        DisplayPhase::PointPhase { point, roll, .. } => {
            if let Some(r) = roll {
                PhaseContext::info(format!(
                    "Point: {point} — rolled {} = {} (need {point} or 7)",
                    r,
                    r.sum()
                ))
            } else {
                PhaseContext::info(format!("Point is {point} — press any key to roll"))
            }
        }

        DisplayPhase::Resolved {
            pass_line_won,
            bankroll,
            ..
        } => {
            let msg = if *pass_line_won {
                format!("Pass Line wins! Bankroll: ${bankroll}")
            } else {
                format!("Pass Line loses. Bankroll: ${bankroll}")
            };
            PhaseContext::info(msg)
        }
    }
}

fn build_game_lines(phase: &DisplayPhase<'_>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match phase {
        DisplayPhase::Betting {
            bankroll,
            lesson_title,
            lesson_level,
            lesson_tip,
        } => {
            lines.push(Line::from(Span::styled(
                format!("🎲  Lesson {lesson_level}: {lesson_title}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(format!("  Bankroll: ${bankroll}")));
            lines.push(Line::from(""));

            // Show first line of lesson text as a tip
            if let Some(first_line) = lesson_tip.lines().nth(1) {
                lines.push(Line::from(Span::styled(
                    format!("  💡 {first_line}"),
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
            }

            lines.push(Line::from(Span::styled(
                "  Place your Pass Line bet ↓",
                Style::default().fg(Color::DarkGray),
            )));
        }

        DisplayPhase::ComeOut {
            bankroll,
            bets,
            roll,
        } => {
            lines.push(Line::from(Span::styled(
                "🎲  Come-Out Roll",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            // Show bets
            for bet in *bets {
                lines.push(Line::from(format!("  📌 {bet}")));
            }
            lines.push(Line::from(""));

            if let Some(r) = roll {
                let sum = r.sum();
                let result = if r.is_natural() {
                    format!("Natural {sum}! 🎉")
                } else if r.is_craps() {
                    format!("Craps {sum}! 💀")
                } else {
                    format!("Point is {sum} 📍")
                };
                lines.push(Line::from(Span::styled(
                    format!("  🎲 {} + {} = {sum}  —  {result}", r.die1(), r.die2()),
                    Style::default()
                        .fg(if r.is_natural() {
                            Color::Green
                        } else if r.is_craps() {
                            Color::Red
                        } else {
                            Color::Yellow
                        })
                        .add_modifier(Modifier::BOLD),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(format!("  Bankroll: ${bankroll}")));
        }

        DisplayPhase::PointPhase {
            bankroll,
            bets,
            point,
            roll,
        } => {
            lines.push(Line::from(Span::styled(
                format!("📍  Point Phase — Point: {point}"),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            for bet in *bets {
                lines.push(Line::from(format!("  📌 {bet}")));
            }
            lines.push(Line::from(""));

            if let Some(r) = roll {
                let sum = r.sum();
                let result = if sum == point.value() {
                    "Point made! 🎯".to_string()
                } else if sum == 7 {
                    "Seven-out! 💀".to_string()
                } else {
                    format!("No decision (need {point} or 7)")
                };
                lines.push(Line::from(Span::styled(
                    format!("  🎲 {} + {} = {sum}  —  {result}", r.die1(), r.die2()),
                    Style::default()
                        .fg(if sum == point.value() {
                            Color::Green
                        } else if sum == 7 {
                            Color::Red
                        } else {
                            Color::White
                        })
                        .add_modifier(Modifier::BOLD),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  Press any key to roll the dice...",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(format!("  Bankroll: ${bankroll}")));
        }

        DisplayPhase::Resolved {
            bankroll,
            results,
            pass_line_won,
            point,
        } => {
            let banner = if *pass_line_won {
                "🎉  You Win!"
            } else {
                "💸  You Lose"
            };
            lines.push(Line::from(Span::styled(
                banner,
                Style::default()
                    .fg(if *pass_line_won {
                        Color::Green
                    } else {
                        Color::Red
                    })
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            if let Some(pt) = point {
                lines.push(Line::from(format!("  Point was: {pt}")));
            } else {
                lines.push(Line::from("  Resolved on come-out"));
            }
            lines.push(Line::from(""));

            for (bet, outcome) in *results {
                let (label, color) = match outcome {
                    BetOutcome::Win(profit) => {
                        (format!("  ✅ {bet} → Win +${profit}"), Color::Green)
                    }
                    BetOutcome::Lose => {
                        (format!("  ❌ {bet} → Lose -${}", bet.amount()), Color::Red)
                    }
                    BetOutcome::Push => (format!("  🤝 {bet} → Push (returned)"), Color::Yellow),
                    BetOutcome::NoAction => (format!("  ⏸️  {bet} → No action"), Color::DarkGray),
                };
                lines.push(Line::from(Span::styled(label, Style::default().fg(color))));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!("  Final bankroll: ${bankroll}")));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Press any key for next round, Q to quit",
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines
}

// ─────────────────────────────────────────────────────────────
//  Input helpers
// ─────────────────────────────────────────────────────────────

/// Wait for a keypress. Returns `false` if Q pressed (quit).
async fn wait_for_keypress_raw() -> Result<RoundOutcome> {
    loop {
        let ev = tokio::task::spawn_blocking(event::read).await??;
        if let Event::Key(key) = ev {
            return Ok(
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q') {
                    RoundOutcome::Quit
                } else {
                    RoundOutcome::Continue
                },
            );
        }
    }
}

/// Wait for continue/quit after a round.
async fn wait_for_continue<B: Backend>(
    _ctx: &mut RenderCtx<'_, B>,
    _event_log: &[GameEvent],
) -> Result<RoundOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    // Drain pending events so we don't accidentally skip
    while event::poll(std::time::Duration::from_millis(50))? {
        let _ = event::read()?;
    }

    wait_for_keypress_raw().await
}

// ─────────────────────────────────────────────────────────────
//  Narration helpers
// ─────────────────────────────────────────────────────────────

/// Number of distinct dice combinations that produce a given sum.
#[instrument(skip_all)]
fn ways_to_roll(sum: u8) -> u8 {
    match sum {
        2 | 12 => 1,
        3 | 11 => 2,
        4 | 10 => 3,
        5 | 9 => 4,
        6 | 8 => 5,
        7 => 6,
        _ => 0,
    }
}

/// Format a bet outcome with dollar amounts for narration.
#[instrument(skip_all)]
fn narrate_outcome(bet: &ActiveBet, outcome: &BetOutcome) -> String {
    match outcome {
        BetOutcome::Win(profit) => {
            format!("  ✅ {} → Win +${profit}", bet)
        }
        BetOutcome::Lose => {
            format!("  ❌ {} → Lose -${}", bet, bet.amount())
        }
        BetOutcome::Push => {
            format!("  🤝 {} → Push (returned)", bet)
        }
        BetOutcome::NoAction => {
            format!("  ⏸️  {} → No action", bet)
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Style
// ─────────────────────────────────────────────────────────────

/// Custom style for craps bet amount prompts.
#[derive(Debug, Clone)]
struct CrapsBetStyle {
    min: u64,
    max: u64,
    bankroll: u64,
}

impl CrapsBetStyle {
    fn new(min: u64, max: u64, bankroll: u64) -> Self {
        Self { min, max, bankroll }
    }
}

impl Default for CrapsBetStyle {
    fn default() -> Self {
        Self {
            min: 5,
            max: 500,
            bankroll: 1000,
        }
    }
}

impl elicitation::style::ElicitationStyle for CrapsBetStyle {
    fn prompt_for_field(
        &self,
        _field_name: &str,
        _field_type: &str,
        _context: &elicitation::style::PromptContext,
    ) -> String {
        format!(
            "Place your Pass Line bet (${}-${}, bankroll: ${}). Enter amount:",
            self.min, self.max, self.bankroll
        )
    }
}
