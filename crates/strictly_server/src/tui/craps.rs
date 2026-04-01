//! Craps TUI game loop.
//!
//! Drives a local craps trainer session entirely within the ratatui terminal.
//! The [`TuiCommunicator`] handles all player decisions via the elicitation
//! framework, so the same interface works for humans and AI agents.
//!
//! Game logic is wired through proof-carrying workflow tools:
//! `execute_place_bets` → `execute_comeout_roll` → `execute_point_roll`.
//! The compiler enforces correct phase ordering via `Established<P>` contracts.

use crate::tui::contracts::{CrapsRoundActive, NoOverflow};
use crate::tui::observable_communicator::ObservableCommunicator;
use crate::tui::tui_communicator::TuiCommunicator;
use crate::tui::typestate_widget::{
    GameEvent, PhaseContext, TypestateGraphWidget, craps_active, craps_edges, craps_nodes,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use elicitation::ElicitCommunicator as _;
use elicitation::Elicitation as _;
use elicitation::Generator as _;
use elicitation::contracts::{And, Established, Prop, both};
use ratatui::{Terminal, backend::Backend, prelude::Widget};
use strictly_craps::{
    ActiveBet, BetOutcome, BetType, ComeOutOutput, CrapsAction, CrapsTable, CrapsTableView,
    DiceRoll, GameSetup, Point, PointRollOutput, execute_comeout_roll, execute_place_bets,
    execute_point_roll,
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
            player_name: &player_name,
            show_typestate_graph,
            nodes: &nodes,
            edges: &edges,
            prompt_rx: prompt_rx.clone(),
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
    let _ = render_craps(
        ctx,
        DisplayPhase::Betting {
            bankroll,
            lesson_title: lesson.lesson_title(),
            lesson_level: lesson.level(),
            lesson_tip: lesson.lesson_text(),
        },
        craps_active(&current_phase),
        event_log,
        Established::<CrapsRoundActive>::assert(),
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

    let _ = render_craps(
        ctx,
        DisplayPhase::ComeOut {
            bankroll: *table.seats()[0].bankroll(),
            bets: &seat_bets[0],
            roll: Some(roll),
        },
        craps_active(&current_phase),
        event_log,
        Established::<CrapsRoundActive>::assert(),
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

            let _ = render_craps(
                ctx,
                DisplayPhase::Resolved {
                    bankroll: *table.seats()[0].bankroll(),
                    results: &seat_results,
                    pass_line_won: pass_won,
                    point: None,
                },
                craps_active(&current_phase),
                event_log,
                Established::<CrapsRoundActive>::assert(),
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
                let _ = render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: None,
                    },
                    craps_active(&current_phase),
                    event_log,
                    Established::<CrapsRoundActive>::assert(),
                )?;

                // Wait for keypress to roll
                event_log.push(GameEvent::story("  Press any key to roll...".to_string()));
                let _ = render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: None,
                    },
                    craps_active(&current_phase),
                    event_log,
                    Established::<CrapsRoundActive>::assert(),
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

                let _ = render_craps(
                    ctx,
                    DisplayPhase::PointPhase {
                        bankroll: *table.seats()[0].bankroll(),
                        bets: &seat_bets[0],
                        point,
                        roll: Some(point_roll),
                    },
                    craps_active(&current_phase),
                    event_log,
                    Established::<CrapsRoundActive>::assert(),
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

                        let _ = render_craps(
                            ctx,
                            DisplayPhase::Resolved {
                                bankroll: *table.seats()[0].bankroll(),
                                results: &seat_results,
                                pass_line_won: pass_won,
                                point: Some(point),
                            },
                            craps_active(&current_phase),
                            event_log,
                            Established::<CrapsRoundActive>::assert(),
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
fn render_craps<B: Backend, P: Prop>(
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

    let pending_prompt = ctx.prompt_rx.borrow().clone();
    let phase_ctx = build_phase_context(&phase).with_pending_prompt(pending_prompt);

    let game_title = format!(" 🎲 Craps — {} 🎲 ", ctx.player_name);
    let game_text = build_game_text(&phase, &pal);
    let story_text = build_craps_story_text(event_log, &pal);

    let content_constraints: Vec<ConstraintJson> = if ctx.show_typestate_graph {
        vec![
            ConstraintJson::Percentage { value: 40 },
            ConstraintJson::Percentage { value: 35 },
            ConstraintJson::Percentage { value: 25 },
        ]
    } else {
        vec![
            ConstraintJson::Percentage { value: 45 },
            ConstraintJson::Percentage { value: 55 },
        ]
    };

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

    let story_node = TuiNode::Widget {
        widget: Box::new(WidgetJson::Paragraph {
            text: ParagraphText::Rich(story_text),
            style: None,
            wrap: true,
            scroll: None,
            alignment: None,
            block: Some(BlockJson {
                borders: BordersJson::All,
                border_type: None,
                title: Some(" Story ".to_string()),
                style: None,
                border_style: Some(border_style.clone()),
                padding: None,
            }),
        }),
    };

    let mut content_children = vec![game_node, story_node];
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

        if ctx.show_typestate_graph {
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
            TypestateGraphWidget::new(ctx.nodes, ctx.edges, active, event_log)
                .with_context(&phase_ctx)
                .render(cols[2], frame.buffer_mut());
        }
    })?;
    Ok(both(game_proof, Established::assert()))
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
        DisplayPhase::Betting {
            bankroll,
            lesson_title,
            lesson_level,
            lesson_tip,
        } => {
            lines.push(styled_line(
                format!("🎲  Lesson {lesson_level}: {lesson_title}"),
                warning_style,
            ));
            lines.push(empty());
            lines.push(plain(format!("  Bankroll: ${bankroll}")));
            lines.push(empty());
            if let Some(first_line) = lesson_tip.lines().nth(1) {
                lines.push(styled_line(
                    format!("  💡 {first_line}"),
                    muted_style.clone(),
                ));
                lines.push(empty());
            }
            lines.push(styled_line(
                "  Place your Pass Line bet ↓".to_string(),
                muted_style,
            ));
        }

        DisplayPhase::ComeOut {
            bankroll,
            bets,
            roll,
        } => {
            lines.push(styled_line(
                "🎲  Come-Out Roll".to_string(),
                highlight_style,
            ));
            lines.push(empty());
            for bet in *bets {
                lines.push(plain(format!("  📌 {bet}")));
            }
            lines.push(empty());
            if let Some(r) = roll {
                let sum = r.sum();
                let result = if r.is_natural() {
                    format!("Natural {sum}! 🎉")
                } else if r.is_craps() {
                    format!("Craps {sum}! 💀")
                } else {
                    format!("Point is {sum} 📍")
                };
                let st = if r.is_natural() {
                    success_style
                } else if r.is_craps() {
                    error_style
                } else {
                    warning_style
                };
                lines.push(styled_line(
                    format!("  🎲 {} + {} = {sum}  —  {result}", r.die1(), r.die2()),
                    st,
                ));
            }
            lines.push(empty());
            lines.push(plain(format!("  Bankroll: ${bankroll}")));
        }

        DisplayPhase::PointPhase {
            bankroll,
            bets,
            point,
            roll,
        } => {
            lines.push(styled_line(
                format!("📍  Point Phase — Point: {point}"),
                warning_style,
            ));
            lines.push(empty());
            for bet in *bets {
                lines.push(plain(format!("  📌 {bet}")));
            }
            lines.push(empty());
            if let Some(r) = roll {
                let sum = r.sum();
                let result = if sum == point.value() {
                    "Point made! 🎯".to_string()
                } else if sum == 7 {
                    "Seven-out! 💀".to_string()
                } else {
                    format!("No decision (need {point} or 7)")
                };
                let st = if sum == point.value() {
                    success_style
                } else if sum == 7 {
                    error_style
                } else {
                    body_style.clone()
                };
                lines.push(styled_line(
                    format!("  🎲 {} + {} = {sum}  —  {result}", r.die1(), r.die2()),
                    st,
                ));
            } else {
                lines.push(styled_line(
                    "  Press any key to roll the dice...".to_string(),
                    muted_style,
                ));
            }
            lines.push(empty());
            lines.push(plain(format!("  Bankroll: ${bankroll}")));
        }

        DisplayPhase::Resolved {
            bankroll,
            results,
            pass_line_won,
            point,
        } => {
            let (banner, banner_st) = if *pass_line_won {
                ("🎉  You Win!", success_style)
            } else {
                ("💸  You Lose", error_style)
            };
            lines.push(styled_line(banner.to_string(), banner_st));
            lines.push(empty());
            if let Some(pt) = point {
                lines.push(plain(format!("  Point was: {pt}")));
            } else {
                lines.push(plain("  Resolved on come-out".to_string()));
            }
            lines.push(empty());
            for (bet, outcome) in *results {
                let (label, st) = match outcome {
                    BetOutcome::Win(profit) => (
                        format!("  ✅ {bet} → Win +${profit}"),
                        StyleJson {
                            fg: Some(pal.success.json.clone()),
                            bg: None,
                            modifiers: vec![],
                        },
                    ),
                    BetOutcome::Lose => (
                        format!("  ❌ {bet} → Lose -${}", bet.amount()),
                        StyleJson {
                            fg: Some(pal.error.json.clone()),
                            bg: None,
                            modifiers: vec![],
                        },
                    ),
                    BetOutcome::Push => (
                        format!("  🤝 {bet} → Push (returned)"),
                        StyleJson {
                            fg: Some(pal.warning.json.clone()),
                            bg: None,
                            modifiers: vec![],
                        },
                    ),
                    BetOutcome::NoAction => (
                        format!("  ⏸️  {bet} → No action"),
                        StyleJson {
                            fg: Some(pal.muted.json.clone()),
                            bg: None,
                            modifiers: vec![],
                        },
                    ),
                };
                lines.push(styled_line(label, st));
            }
            lines.push(empty());
            lines.push(plain(format!("  Final bankroll: ${bankroll}")));
            lines.push(empty());
            lines.push(styled_line(
                "  Press any key for next round, Q to quit".to_string(),
                StyleJson {
                    fg: Some(pal.muted.json.clone()),
                    bg: None,
                    modifiers: vec![],
                },
            ));
        }
    }

    TextJson {
        lines,
        style: None,
        alignment: None,
    }
}

/// Builds a story pane [`TextJson`] from the craps event log.
fn build_craps_story_text(
    event_log: &[GameEvent],
    pal: &crate::tui::palette::GamePalette,
) -> elicit_ratatui::TextJson {
    use crate::tui::ratatui_color_to_json;
    use elicit_ratatui::{LineJson, SpanJson, StyleJson, TextJson};

    let lines: Vec<LineJson> = event_log
        .iter()
        .map(|ev| LineJson {
            spans: vec![SpanJson {
                content: ev.text.clone(),
                style: Some(StyleJson {
                    fg: Some(ratatui_color_to_json(ev.color, pal)),
                    bg: None,
                    modifiers: vec![],
                }),
            }],
            style: None,
            alignment: None,
        })
        .collect();

    TextJson {
        lines,
        style: None,
        alignment: None,
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

    let (chat_tx, _chat_rx) = chat_channel();
    let (prompt_tx, prompt_rx) = watch::channel(None::<String>);
    let nodes = craps_nodes();
    let edges = craps_edges();
    let dice = DiceRoll::random_generator(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    let mut event_log: Vec<GameEvent> = Vec::new();

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

        let mut ctx = RenderCtx {
            terminal,
            player_name: &player_name,
            show_typestate_graph,
            nodes: &nodes,
            edges: &edges,
            prompt_rx: prompt_rx.clone(),
        };

        // ── Collect bets from all seats ──────────────────────
        let lesson = table.seats()[0].lesson();
        let mut current_phase = "Betting".to_string();

        event_log.push(GameEvent::story(format!(
            "🂠  New round — your bankroll: ${bankroll}"
        )));

        let _ = render_craps(
            &mut ctx,
            DisplayPhase::Betting {
                bankroll,
                lesson_title: lesson.lesson_title(),
                lesson_level: lesson.level(),
                lesson_tip: lesson.lesson_text(),
            },
            craps_active(&current_phase),
            &event_log,
            Established::<CrapsRoundActive>::assert(),
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

        let _ = render_craps(
            &mut ctx,
            DisplayPhase::ComeOut {
                bankroll: *table.seats()[0].bankroll(),
                bets: &all_seat_bets[0],
                roll: Some(roll),
            },
            craps_active(&current_phase),
            &event_log,
            Established::<CrapsRoundActive>::assert(),
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

                let _ = render_craps(
                    &mut ctx,
                    DisplayPhase::Resolved {
                        bankroll: *table.seats()[0].bankroll(),
                        results: &seat_results,
                        pass_line_won: pass_won,
                        point: None,
                    },
                    craps_active(&current_phase),
                    &event_log,
                    Established::<CrapsRoundActive>::assert(),
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
                    let _ = render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: None,
                        },
                        craps_active(&current_phase),
                        &event_log,
                        Established::<CrapsRoundActive>::assert(),
                    )?;

                    event_log.push(GameEvent::story("  Press any key to roll...".to_string()));
                    let _ = render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: None,
                        },
                        craps_active(&current_phase),
                        &event_log,
                        Established::<CrapsRoundActive>::assert(),
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

                    let _ = render_craps(
                        &mut ctx,
                        DisplayPhase::PointPhase {
                            bankroll: *table.seats()[0].bankroll(),
                            bets: &all_seat_bets[0],
                            point,
                            roll: Some(point_roll),
                        },
                        craps_active(&current_phase),
                        &event_log,
                        Established::<CrapsRoundActive>::assert(),
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

                            let _ = render_craps(
                                &mut ctx,
                                DisplayPhase::Resolved {
                                    bankroll: *table.seats()[0].bankroll(),
                                    results: &seat_results,
                                    pass_line_won: pass_won,
                                    point: Some(point),
                                },
                                craps_active(&current_phase),
                                &event_log,
                                Established::<CrapsRoundActive>::assert(),
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
