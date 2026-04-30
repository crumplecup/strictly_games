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
use crate::tui::typestate_widget::{GameEvent, craps_active, craps_edges, craps_nodes};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use elicitation::ElicitCommunicator as _;
use elicitation::Elicitation as _;
use elicitation::Generator as _;
use ratatui::{Terminal, backend::Backend};
use strictly_craps::{
    ActiveBet, BetOutcome, BetType, ComeOutOutput, CrapsAction, CrapsTable, CrapsTableView,
    DiceRoll, GameSetup, Point, PointRollOutput, execute_comeout_roll, execute_place_bets,
    execute_point_roll,
};
use tokio::sync::watch;
use tracing::{info, instrument, warn};

use crate::session::DialogueEntry;

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
    show_typestate_graph: bool,
    nodes: &'a [crate::tui::typestate_widget::NodeDef],
    edges: &'a [crate::tui::typestate_widget::EdgeDef],
    dialogue: &'a [DialogueEntry],
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

    let (prompt_tx, _) = watch::channel(None::<String>);
    let comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx);
    let nodes = craps_nodes();
    let edges = craps_edges();
    let dice = DiceRoll::random_generator(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
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
            show_typestate_graph,
            nodes: &nodes,
            edges: &edges,
            dialogue: &[],
        };

        let outcome = run_single_round(&mut ctx, &mut table, &comm, &mut event_log, &dice).await?;

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
    rng: &impl elicitation::Generator<Target = DiceRoll>,
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
    let roll = rng.generate();
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
                let point_roll = rng.generate();
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
    use crate::tui::contracts::{CrapsUiConsistent, render_resize_prompt, verified_draw};
    use crate::tui::game_ir::{EventLog, GraphParams, craps_to_verified_tree};
    use elicit_ratatui::RatatuiBackend;
    use elicit_ui::{UiTreeRenderer as _, Viewport};
    use elicitation::contracts::Established;
    use strictly_craps::CrapsDisplayMode;

    let view = craps_state_view_from_phase(&phase);
    let craps_graph_nodes = if ctx.show_typestate_graph {
        ctx.nodes
    } else {
        &[]
    };
    let craps_graph_edges = if ctx.show_typestate_graph {
        ctx.edges
    } else {
        &[]
    };
    let log = EventLog {
        events: event_log,
        dialogue: ctx.dialogue,
    };
    let graph = GraphParams {
        nodes: craps_graph_nodes,
        edges: craps_graph_edges,
        active,
    };

    ctx.terminal.draw(|f| {
        let area = f.area();
        let viewport = Viewport::new(area.width as u32, area.height as u32);
        let tree = craps_to_verified_tree(&view, &CrapsDisplayMode::Table, &log, &graph, viewport);
        let backend = RatatuiBackend::new();
        let (tui_node, _stats, render_proof) = backend
            .render(&tree)
            .unwrap_or_else(|e| panic!("RatatuiBackend::render failed: {e}"));
        let _: Established<CrapsUiConsistent> = Established::prove(&render_proof);
        verified_draw(f, area, &tui_node).unwrap_or_else(|e| {
            render_resize_prompt(f, &e);
            Established::assert()
        });
    })?;
    Ok(())
}

/// Build a [`CrapsStateView`] snapshot from the current display phase.
fn craps_state_view_from_phase(phase: &DisplayPhase<'_>) -> crate::games::craps::CrapsStateView {
    use crate::games::craps::CrapsStateView;
    match phase {
        DisplayPhase::Betting {
            bankroll,
            lesson_title,
            lesson_level,
            lesson_tip,
        } => CrapsStateView {
            phase: format!("Betting — Lesson {lesson_level}"),
            bankroll: *bankroll,
            description: format!("{lesson_title}\n{lesson_tip}"),
            active_bets: vec![],
            dice_roll: None,
            point: None,
            is_terminal: false,
        },

        DisplayPhase::ComeOut {
            bankroll,
            bets,
            roll,
        } => CrapsStateView {
            phase: "ComeOut".to_string(),
            bankroll: *bankroll,
            description: match roll {
                Some(r) => {
                    let sum = r.sum();
                    if r.is_natural() {
                        format!("Natural {sum}! — 7 or 11 on come-out wins Pass Line")
                    } else if r.is_craps() {
                        format!("Craps {sum}! — 2, 3, or 12 on come-out loses Pass Line")
                    } else {
                        format!("Point is {sum} — puck ON")
                    }
                }
                None => "Awaiting come-out roll…".to_string(),
            },
            active_bets: bets.iter().map(|b| b.to_string()).collect(),
            dice_roll: roll.map(|r| format!("{} + {} = {}", r.die1(), r.die2(), r.sum())),
            point: None,
            is_terminal: false,
        },

        DisplayPhase::PointPhase {
            bankroll,
            bets,
            point,
            roll,
        } => CrapsStateView {
            phase: format!("PointPhase — Point: {point}"),
            bankroll: *bankroll,
            description: match roll {
                Some(r) => {
                    let sum = r.sum();
                    if sum == point.value() {
                        format!("Point {point} made! 🎯")
                    } else if sum == 7 {
                        format!("Seven-out! 💀 (need {point} or 7)")
                    } else {
                        format!("No decision — need {point} or 7")
                    }
                }
                None => format!("Point is {point} — press any key to roll"),
            },
            active_bets: bets.iter().map(|b| b.to_string()).collect(),
            dice_roll: roll.map(|r| format!("{} + {} = {}", r.die1(), r.die2(), r.sum())),
            point: Some(point.to_string()),
            is_terminal: false,
        },

        DisplayPhase::Resolved {
            bankroll,
            results,
            pass_line_won,
            point,
        } => CrapsStateView {
            phase: "Resolved".to_string(),
            bankroll: *bankroll,
            description: if *pass_line_won {
                "Pass Line wins! 🎉  Press any key for next round, Q to quit".to_string()
            } else {
                "Pass Line loses. 💸  Press any key for next round, Q to quit".to_string()
            },
            active_bets: results
                .iter()
                .map(|(b, o)| format!("{b} → {o:?}"))
                .collect(),
            dice_roll: None,
            point: point.map(|p| p.to_string()),
            is_terminal: false,
        },
    }
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

// ─────────────────────────────────────────────────────────────
//  Multi-player support
// ─────────────────────────────────────────────────────────────

use crate::tui::chat_widget::{Participant, chat_channel};
use crate::tui::mcp_communicator::LlmElicitCommunicator;
use crate::{PlayerKind, PlayerSlot};
use strictly_craps::AgentPersonality;

/// A co-player slot for multi-seat craps, pairing a player with a personality.
#[derive(Debug, Clone)]
pub struct CrapsCoPlayer {
    /// Base player slot (name, bankroll, kind).
    pub slot: PlayerSlot,
    /// Personality governing the AI's betting strategy.
    pub personality: AgentPersonality,
}

/// Type-erased seat communicator for craps — dispatches to human TUI or AI LLM.
enum CrapsSeatComm {
    Human {
        comm: ObservableCommunicator<TuiCommunicator>,
    },
    Agent {
        comm: ObservableCommunicator<LlmElicitCommunicator>,
        personality: AgentPersonality,
    },
}

/// Elicits a valid bet (min..=max) from any communicator.
///
/// Returns `None` only if the communicator signals a cancellation.
#[instrument(skip(comm))]
async fn elicit_craps_bet<C: elicitation::ElicitCommunicator>(
    comm: &C,
    table_min: u64,
    table_max: u64,
    bankroll: u64,
) -> Option<u64> {
    let max_bet = bankroll.min(table_max);
    let styled =
        comm.with_style::<u64, CrapsBetStyle>(CrapsBetStyle::new(table_min, max_bet, bankroll));
    loop {
        match u64::elicit(&styled).await {
            Ok(v) if v >= table_min && v <= max_bet => return Some(v),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// Elicits a bet from an agent, allowing exploration of table state first.
///
/// The agent sees both commit (PlaceBet / Done) and explore variants via
/// [`CrapsAction`].  Explore selections build a [`CrapsTableView`] snapshot
/// and cache the description in a [`KnowledgeCache`] that is prepended to
/// every subsequent prompt.  When the agent commits to `PlaceBet`, the
/// actual bet amount is elicited via the styled `u64` prompt.  `Done`
/// returns `None`.
#[instrument(skip(comm, event_log))]
async fn elicit_agent_craps_bet<C: elicitation::ElicitCommunicator + Clone>(
    comm: &C,
    table_min: u64,
    table_max: u64,
    bankroll: u64,
    seat_name: &str,
    event_log: &mut Vec<GameEvent>,
) -> Option<u64> {
    use crate::tui::contextual_communicator::{ContextualCommunicator, knowledge_cache};

    let knowledge = knowledge_cache();
    let ctx_comm = ContextualCommunicator::new(comm.clone(), knowledge.clone());

    loop {
        let action = CrapsAction::elicit(&ctx_comm).await.ok()?;

        match action {
            CrapsAction::PlaceBet => {
                return elicit_craps_bet(&ctx_comm, table_min, table_max, bankroll).await;
            }
            CrapsAction::Done => return None,
            _ => {
                let category = action.explore_category().unwrap_or("unknown");
                let view = CrapsTableView::from_betting(bankroll);
                let description = view
                    .describe_category(category)
                    .unwrap_or_else(|| "No information available".to_string());

                let narration = match action {
                    CrapsAction::ViewPoint => "checks the point",
                    CrapsAction::ViewActiveBets => "reviews their bets",
                    CrapsAction::ViewOtherBets => "looks at other bets",
                    CrapsAction::ViewRollHistory => "checks roll history",
                    CrapsAction::ViewBankroll => "checks bankroll",
                    _ => "explores",
                };
                event_log.push(GameEvent::story(format!("  🔍 {seat_name} {narration}")));

                knowledge
                    .lock()
                    .unwrap()
                    .push(format!("[{category}] {description}"));

                let _ = ctx_comm
                    .send_prompt(&format!("[Table State — {category}] {description}"))
                    .await;
            }
        }
    }
}

/// Runs a multi-seat craps session with one human and AI co-players.
///
/// All players bet independently on each round, share the same dice rolls,
/// and have their results displayed in the story pane. The human player
/// controls the flow (keypresses to roll, quit decisions).
///
/// AI co-players each have an [`AgentPersonality`] that shapes their
/// system prompt and betting behaviour.
#[instrument(skip_all, fields(num_players = co_players.len() + 1))]
pub async fn run_multi_craps_session<B: Backend>(
    terminal: &mut Terminal<B>,
    player_name: String,
    initial_bankroll: u64,
    co_players: Vec<CrapsCoPlayer>,
    show_typestate_graph: bool,
) -> Result<CrapsSessionOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    info!("Starting multi-player craps session");

    let (chat_tx, mut chat_rx) = chat_channel();
    let (prompt_tx, _) = watch::channel(None::<String>);
    let nodes = craps_nodes();
    let edges = craps_edges();
    let dice = DiceRoll::random_generator(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    let mut event_log: Vec<GameEvent> = Vec::new();
    let mut dialogue: Vec<DialogueEntry> = Vec::new();

    // ── Build seat communicators ──────────────────────────────
    let mut seat_comms: Vec<CrapsSeatComm> = Vec::new();
    let mut seat_names: Vec<String> = Vec::new();

    // Seat 0: human
    let human_comm = ObservableCommunicator::new(TuiCommunicator::new(), prompt_tx)
        .with_chat(chat_tx.clone(), Participant::Human);
    seat_comms.push(CrapsSeatComm::Human { comm: human_comm });
    seat_names.push(player_name.clone());

    // AI co-players
    for cp in &co_players {
        let (agent_prompt_tx, _agent_prompt_rx) = watch::channel(None::<String>);
        match &cp.slot.kind {
            PlayerKind::Agent(config) => match LlmElicitCommunicator::new(config) {
                Ok(base) => {
                    let base = base.with_system_prompt(cp.personality.system_prompt());
                    let participant = Participant::Agent(cp.slot.name.clone());
                    let comm = ObservableCommunicator::new(base, agent_prompt_tx)
                        .with_chat(chat_tx.clone(), participant);
                    seat_comms.push(CrapsSeatComm::Agent {
                        comm,
                        personality: cp.personality,
                    });
                    seat_names.push(cp.slot.name.clone());
                }
                Err(e) => {
                    warn!(
                        agent = %cp.slot.name,
                        error = %e,
                        "Failed to create agent communicator — skipping"
                    );
                }
            },
            PlayerKind::Human => {
                warn!("Multi-seat craps only supports one human — skipping extra");
            }
        }
    }

    // ── Initialize table ─────────────────────────────────────
    let mut table = CrapsTable::new(3, 5, 500);

    // Human seat
    let seat =
        strictly_craps::CrapsSeat::new(player_name.clone(), initial_bankroll).with_shooter(true);
    table.add_seat(seat);

    // AI seats
    for cp in &co_players {
        let ai_seat = strictly_craps::CrapsSeat::new(cp.slot.name.clone(), cp.slot.bankroll);
        table.add_seat(ai_seat);
    }

    event_log.push(GameEvent::story(format!(
        "🎲  Welcome to Craps! {} players at the table",
        seat_comms.len()
    )));
    for (i, name) in seat_names.iter().enumerate() {
        let bankroll = table.seats()[i].bankroll();
        if i == 0 {
            event_log.push(GameEvent::story(format!("  🧑 {name} (you) — ${bankroll}")));
        } else {
            let personality = match &seat_comms[i] {
                CrapsSeatComm::Agent { personality, .. } => personality.label(),
                _ => "Unknown",
            };
            event_log.push(GameEvent::story(format!(
                "  🤖 {name} ({personality}) — ${bankroll}"
            )));
        }
    }

    // ── Main game loop ───────────────────────────────────────
    loop {
        let bankroll = *table.seats()[0].bankroll();
        if bankroll == 0 {
            info!("Human bankroll exhausted — ending session");
            return Ok(CrapsSessionOutcome::Busted);
        }

        // Drain any pending chat messages into the dialogue log.
        while let Ok(msg) = chat_rx.try_recv() {
            dialogue.push(DialogueEntry {
                role: msg.participant.display_name().to_string(),
                text: msg.text,
            });
        }

        let mut ctx = RenderCtx {
            terminal,
            show_typestate_graph,
            nodes: &nodes,
            edges: &edges,
            dialogue: &dialogue,
        };

        // ── Collect bets from all seats ──────────────────────
        let lesson = table.seats()[0].lesson();
        let mut current_phase = "Betting".to_string();

        event_log.push(GameEvent::story(format!(
            "🂠  New round — your bankroll: ${bankroll}"
        )));

        render_craps(
            &mut ctx,
            DisplayPhase::Betting {
                bankroll,
                lesson_title: lesson.lesson_title(),
                lesson_level: lesson.level(),
                lesson_tip: lesson.lesson_text(),
            },
            craps_active(&current_phase),
            &event_log,
        )?;

        // Human bet
        let human_bet = {
            let max_bet = bankroll.min(table.table_max());
            let CrapsSeatComm::Human { ref comm, .. } = seat_comms[0] else {
                unreachable!("seat 0 is always human");
            };
            match elicit_craps_bet(comm, table.table_min(), max_bet, bankroll).await {
                Some(v) => v,
                None => {
                    let final_bankroll = *table.seats()[0].bankroll();
                    return Ok(if final_bankroll > 0 {
                        CrapsSessionOutcome::CashedOut(final_bankroll)
                    } else {
                        CrapsSessionOutcome::Abandoned
                    });
                }
            }
        };

        // Deduct human bet
        table
            .seat_mut(0)
            .expect("seat 0")
            .deduct_wagers(human_bet)
            .map_err(anyhow::Error::msg)?;

        event_log.push(GameEvent::story(format!(
            "💰  You bet ${human_bet} on Pass Line"
        )));

        // AI bets — each agent decides independently
        let mut all_seat_bets: Vec<Vec<ActiveBet>> = Vec::new();
        all_seat_bets.push(vec![ActiveBet::new(BetType::PassLine, human_bet)]);

        for i in 1..seat_comms.len() {
            let ai_bankroll = *table.seats()[i].bankroll();
            if ai_bankroll == 0 {
                event_log.push(GameEvent::story(format!(
                    "  🤖 {} is busted — sitting out",
                    seat_names[i]
                )));
                all_seat_bets.push(vec![]);
                continue;
            }

            let ai_bet = match &seat_comms[i] {
                CrapsSeatComm::Agent { comm, .. } => {
                    let max = ai_bankroll.min(table.table_max());
                    match elicit_agent_craps_bet(
                        comm,
                        table.table_min(),
                        max,
                        ai_bankroll,
                        &seat_names[i],
                        &mut event_log,
                    )
                    .await
                    {
                        Some(v) => v,
                        None => table.table_min().min(ai_bankroll),
                    }
                }
                CrapsSeatComm::Human { .. } => unreachable!(),
            };

            if let Err(e) = table
                .seat_mut(i)
                .expect("seat exists")
                .deduct_wagers(ai_bet)
            {
                warn!(seat = i, error = %e, "AI bet deduction failed");
                all_seat_bets.push(vec![]);
                continue;
            }

            let personality_label = match &seat_comms[i] {
                CrapsSeatComm::Agent { personality, .. } => personality.label(),
                _ => "",
            };
            event_log.push(GameEvent::story(format!(
                "  🤖 {} ({}) bets ${ai_bet} on Pass Line",
                seat_names[i], personality_label
            )));
            all_seat_bets.push(vec![ActiveBet::new(BetType::PassLine, ai_bet)]);
        }

        event_log.push(GameEvent::proof("BetsPlaced"));

        // ── Execute typestate flow ───────────────────────────
        let setup = GameSetup::new(seat_comms.len(), table.max_odds());
        let betting_state = setup.start_betting(table.bankroll_vec());
        let (comeout_state, bets_proof) =
            execute_place_bets(betting_state, all_seat_bets.clone()).map_err(anyhow::Error::msg)?;

        current_phase = "ComeOut".to_string();
        event_log.push(GameEvent::phase_change("Betting", "ComeOut"));

        // ── Come-out roll ────────────────────────────────────
        let roll = dice.generate();
        let sum = roll.sum();

        render_craps(
            &mut ctx,
            DisplayPhase::ComeOut {
                bankroll: *table.seats()[0].bankroll(),
                bets: &all_seat_bets[0],
                roll: Some(roll),
            },
            craps_active(&current_phase),
            &event_log,
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

                // Settle all seats
                let results = table.settle_round(&all_seat_bets, resolved.final_roll(), None, true);

                // Narrate human result
                if pass_won {
                    table
                        .seat_mut(0)
                        .expect("seat 0")
                        .credit_winnings(human_bet * 2);
                    let natural_word = if sum == 7 { "Seven" } else { "Yo-leven" };
                    event_log.push(GameEvent::story(format!(
                        "✨  Natural {natural_word}! — Pass Line wins ${human_bet}"
                    )));
                } else {
                    let craps_word = match sum {
                        2 => "Snake Eyes",
                        3 => "Ace-Deuce",
                        12 => "Boxcars",
                        _ => "Craps",
                    };
                    event_log.push(GameEvent::story(format!(
                        "💀  {craps_word} ({sum})! — Pass Line loses ${human_bet}"
                    )));
                }

                // Narrate AI results
                for i in 1..seat_comms.len() {
                    let ai_bet_amount = all_seat_bets[i].first().map(|b| b.amount()).unwrap_or(0);
                    if ai_bet_amount == 0 {
                        continue;
                    }
                    if pass_won {
                        table
                            .seat_mut(i)
                            .expect("seat exists")
                            .credit_winnings(ai_bet_amount * 2);
                        event_log.push(GameEvent::story(format!(
                            "  🤖 {} wins +${ai_bet_amount}",
                            seat_names[i]
                        )));
                    } else {
                        event_log.push(GameEvent::story(format!(
                            "  🤖 {} loses -${ai_bet_amount}",
                            seat_names[i]
                        )));
                    }
                }

                let seat_results = if !results.is_empty() {
                    results[0]
                        .outcomes()
                        .iter()
                        .map(|(b, o)| (b.clone(), *o))
                        .collect()
                } else {
                    vec![]
                };

                event_log.push(GameEvent::phase_change("ComeOut", "Resolved"));
                event_log.push(GameEvent::proof("RoundSettled"));

                render_craps(
                    &mut ctx,
                    DisplayPhase::Resolved {
                        bankroll: *table.seats()[0].bankroll(),
                        results: &seat_results,
                        pass_line_won: pass_won,
                        point: None,
                    },
                    craps_active(&current_phase),
                    &event_log,
                )?;

                for i in 0..seat_comms.len() {
                    table.seat_mut(i).expect("seat").record_round();
                }

                let new_bankroll = *table.seats()[0].bankroll();
                let net: i64 = if pass_won {
                    human_bet as i64
                } else {
                    -(human_bet as i64)
                };
                let sign = if net >= 0 { "+" } else { "" };
                event_log.push(GameEvent::result(format!(
                    "Round over: {sign}${} · bankroll: ${new_bankroll}",
                    net.unsigned_abs()
                )));

                let outcome = wait_for_continue(&mut ctx, &event_log).await?;
                if matches!(outcome, RoundOutcome::Quit) {
                    let final_bankroll = *table.seats()[0].bankroll();
                    return Ok(if final_bankroll > 0 {
                        CrapsSessionOutcome::CashedOut(final_bankroll)
                    } else {
                        CrapsSessionOutcome::Abandoned
                    });
                }
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

                // ── Point phase rolls ────────────────────────
                let mut current_proof = point_proof;
                let mut roll_count: u32 = 0;
                loop {
                    render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: None,
                        },
                        craps_active(&current_phase),
                        &event_log,
                    )?;

                    event_log.push(GameEvent::story("  Press any key to roll...".to_string()));
                    render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: None,
                        },
                        craps_active(&current_phase),
                        &event_log,
                    )?;

                    if matches!(wait_for_keypress_raw().await?, RoundOutcome::Quit) {
                        let final_bankroll = *table.seats()[0].bankroll();
                        return Ok(if final_bankroll > 0 {
                            CrapsSessionOutcome::CashedOut(final_bankroll)
                        } else {
                            CrapsSessionOutcome::Abandoned
                        });
                    }

                    roll_count += 1;
                    let point_roll = dice.generate();
                    let point_sum = point_roll.sum();
                    event_log.push(GameEvent::story(format!(
                        "🎲  Roll #{roll_count}: {} + {} = {point_sum}",
                        point_roll.die1(),
                        point_roll.die2(),
                    )));

                    render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: Some(point_roll),
                        },
                        craps_active(&current_phase),
                        &event_log,
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
                                &all_seat_bets,
                                resolved.final_roll(),
                                Some(point),
                                false,
                            );

                            // Narrate human result
                            if pass_won {
                                table
                                    .seat_mut(0)
                                    .expect("seat 0")
                                    .credit_winnings(human_bet * 2);
                                event_log.push(GameEvent::story(format!(
                                    "🎯  Point {point} hit on roll #{roll_count}! \
                                     Pass Line wins ${human_bet}"
                                )));
                            } else {
                                event_log.push(GameEvent::story(format!(
                                    "💀  Seven-out on roll #{roll_count}! \
                                     Pass Line loses ${human_bet}"
                                )));
                            }

                            // Narrate AI results
                            for i in 1..seat_comms.len() {
                                let ai_bet_amount =
                                    all_seat_bets[i].first().map(|b| b.amount()).unwrap_or(0);
                                if ai_bet_amount == 0 {
                                    continue;
                                }
                                if pass_won {
                                    table
                                        .seat_mut(i)
                                        .expect("seat exists")
                                        .credit_winnings(ai_bet_amount * 2);
                                    event_log.push(GameEvent::story(format!(
                                        "  🤖 {} wins +${ai_bet_amount}",
                                        seat_names[i]
                                    )));
                                } else {
                                    event_log.push(GameEvent::story(format!(
                                        "  🤖 {} loses -${ai_bet_amount}",
                                        seat_names[i]
                                    )));
                                }
                            }

                            let seat_results = if !results.is_empty() {
                                results[0]
                                    .outcomes()
                                    .iter()
                                    .map(|(b, o)| (b.clone(), *o))
                                    .collect()
                            } else {
                                vec![]
                            };

                            event_log.push(GameEvent::phase_change("PointPhase", "Resolved"));
                            event_log.push(GameEvent::proof("RoundSettled"));

                            render_craps(
                                &mut ctx,
                                DisplayPhase::Resolved {
                                    bankroll: *table.seats()[0].bankroll(),
                                    results: &seat_results,
                                    pass_line_won: pass_won,
                                    point: Some(point),
                                },
                                craps_active(&current_phase),
                                &event_log,
                            )?;

                            for i in 0..seat_comms.len() {
                                table.seat_mut(i).expect("seat").record_round();
                            }

                            let new_bankroll = *table.seats()[0].bankroll();
                            let net: i64 = if pass_won {
                                human_bet as i64
                            } else {
                                -(human_bet as i64)
                            };
                            let sign = if net >= 0 { "+" } else { "" };
                            event_log.push(GameEvent::result(format!(
                                "Round over: {sign}${} · bankroll: ${new_bankroll}",
                                net.unsigned_abs()
                            )));

                            let outcome = wait_for_continue(&mut ctx, &event_log).await?;
                            if matches!(outcome, RoundOutcome::Quit) {
                                let final_bankroll = *table.seats()[0].bankroll();
                                return Ok(if final_bankroll > 0 {
                                    CrapsSessionOutcome::CashedOut(final_bankroll)
                                } else {
                                    CrapsSessionOutcome::Abandoned
                                });
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Check lesson advancement for all seats
        for (i, name) in seat_names.iter().enumerate().take(seat_comms.len()) {
            if table.seats()[i].lesson().can_advance()
                && let Some(seat) = table.seat_mut(i)
                && seat.advance_round()
            {
                let new_title = seat.lesson().lesson_title();
                let new_level = seat.lesson().level();
                if i == 0 {
                    event_log.push(GameEvent::story(format!(
                        "🎓  Level up! Lesson {new_level}: {new_title}"
                    )));
                } else {
                    event_log.push(GameEvent::story(format!(
                        "  🤖 {name} leveled up to Lesson {new_level}"
                    )));
                }
            }
        }
    }
}
