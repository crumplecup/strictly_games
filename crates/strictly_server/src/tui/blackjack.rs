//! Blackjack TUI game loop — human + MCP agent mode.
//!
//! Spawns the HTTP game server and an agent subprocess.  The human player
//! drives their own blackjack hand via keyboard (calling MCP tools directly),
//! while the agent plays its own hand concurrently via its subprocess.
//! The TUI shows both hands and the agent's chat log.

use crate::tui::contracts::{min_typestate_width, render_resize_prompt, verify_typestate_readable};
use crate::tui::typestate_widget::GameEvent;
use crate::tui::typestate_widget::{blackjack_edges, blackjack_nodes};
use anyhow::Result;
use elicitation::Elicitation as _;
use ratatui::{Terminal, backend::Backend};
use tokio::time::Duration;
use tracing::{info, instrument};

/// Height of the dedicated prompt pane at the bottom of the game layout.
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

/// Run a blackjack session with the human playing via keyboard and zero or
/// more AI agents playing concurrently via MCP.
///
/// `players` must have the human slot first (`PlayerKind::Human`), followed
/// by any number of agent slots (`PlayerKind::Agent`).  Each agent gets its
/// own independent session on the shared HTTP server and is rendered in its
/// own panel to the right of the human panel.
///
/// `fallback_agent_config` is used when an agent slot carries no explicit
/// config path.
#[instrument(skip_all, fields(port, num_players = players.len(), show_typestate_graph))]
pub async fn run_blackjack_mcp_session<B: Backend>(
    terminal: &mut Terminal<B>,
    players: Vec<crate::PlayerSlot>,
    port: u16,
    fallback_agent_config: std::path::PathBuf,
    show_typestate_graph: bool,
) -> Result<BlackjackSessionOutcome>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    use crate::session::DialogueEntry;
    use crate::session::SharedTableSeatView;
    use crate::tui::chat_widget::ChatWidget;
    use crate::tui::rest_client::{BlackjackObserver, BlackjackTool, HumanBlackjackClient};
    use crate::tui::standalone::{GameMode, ProcessGuards, spawn_agent, spawn_server};
    use crate::tui::typestate_widget::{TypestateGraphWidget, blackjack_active};
    use crate::tui::{ChatMessage, Participant};
    use crate::{PlayerKind, PlayerSlot};
    use crossterm::event::{Event, KeyCode};
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::prelude::Widget;
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    const HUMAN_SESSION: &str = "human_bj";

    // ── Partition slots ───────────────────────────────────────────────────────
    let human_slot = players
        .iter()
        .find(|s| matches!(s.kind, PlayerKind::Human))
        .cloned()
        .unwrap_or_else(|| PlayerSlot {
            name: "You".to_string(),
            bankroll: 1_000,
            kind: PlayerKind::Human,
        });

    let agent_slots: Vec<_> = players
        .iter()
        .filter(|s| matches!(s.kind, PlayerKind::Agent(_)))
        .cloned()
        .collect();

    let player_name = human_slot.name.clone();
    let initial_bankroll = human_slot.bankroll;

    info!(
        player = %player_name,
        bankroll = initial_bankroll,
        num_agents = agent_slots.len(),
        "Starting blackjack MCP session"
    );

    let num_seats = players.len() as u64;
    let server_url = format!("http://localhost:{}", port);

    // ── Spawn server ──────────────────────────────────────────────────────────
    let server = spawn_server(port).await?;

    // ── Connect human MCP session FIRST to initialise the shared table ────────
    let human = HumanBlackjackClient::connect(&server_url).await?;
    human
        .call_tool(
            "blackjack_deal",
            serde_json::json!({
                "initial_bankroll": initial_bankroll,
                "session_id": HUMAN_SESSION,
                "num_seats": num_seats,
                "player_name": player_name
            }),
        )
        .await?;

    // ── Spawn one agent subprocess per seated agent (after table is init'd) ───
    let mut agent_children = Vec::with_capacity(agent_slots.len());
    let mut agent_session_ids: Vec<String> = Vec::with_capacity(agent_slots.len());

    for (idx, slot) in agent_slots.iter().enumerate() {
        let session_id = format!("agent_bj_{idx}");
        let config_path = match &slot.kind {
            PlayerKind::Agent(cfg) => cfg
                .config_path()
                .clone()
                .unwrap_or_else(|| fallback_agent_config.clone()),
            PlayerKind::Human => fallback_agent_config.clone(),
        };
        let child = spawn_agent(
            port,
            config_path,
            GameMode::Blackjack {
                bankroll: slot.bankroll,
                session_id: session_id.clone(),
            },
        )
        .await?;
        agent_children.push(child);
        agent_session_ids.push(session_id);
    }

    let _guards = ProcessGuards::many(server, agent_children);

    // ── Observers ─────────────────────────────────────────────────────────────
    let human_observer = BlackjackObserver::new(server_url.clone(), HUMAN_SESSION.to_string());
    let agent_observers: Vec<BlackjackObserver> = agent_session_ids
        .iter()
        .map(|sid| BlackjackObserver::new(server_url.clone(), sid.clone()))
        .collect();

    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();

    // Per-agent mutable state
    let mut agent_dialogues: Vec<Vec<DialogueEntry>> = vec![Vec::new(); agent_slots.len()];
    let mut available_tools: Vec<BlackjackTool> = Vec::new();
    let mut tool_refresh_counter: u8 = 0;
    let mut event_log: Vec<GameEvent> = vec![GameEvent::story(format!(
        "🃏  Blackjack — {player_name} joined (bankroll: ${initial_bankroll})"
    ))];
    let mut prev_human_phase = "idle".to_string();
    let mut prev_agent_phases: Vec<String> = vec!["idle".to_string(); agent_slots.len()];

    loop {
        // ── Poll state ────────────────────────────────────────────────────────
        let idle_state = SharedTableSeatView {
            phase: "idle".to_string(),
            bankroll: 0,
            description: "Connecting...".to_string(),
            is_terminal: false,
        };

        let human_state = human_observer
            .get_blackjack_state()
            .await
            .unwrap_or_else(|_| idle_state.clone());

        let mut agent_states: Vec<SharedTableSeatView> = Vec::with_capacity(agent_observers.len());
        for (i, obs) in agent_observers.iter().enumerate() {
            let state = obs
                .get_blackjack_state()
                .await
                .unwrap_or_else(|_| idle_state.clone());
            agent_states.push(state);

            if let Ok(entries) = obs.get_dialogue().await {
                agent_dialogues[i] = entries;
            }
        }

        // Refresh available tools every ~500ms (every 2 loop iterations)
        tool_refresh_counter = tool_refresh_counter.wrapping_add(1);
        if tool_refresh_counter.is_multiple_of(2)
            && let Ok(tools) = human.list_blackjack_tools().await
        {
            available_tools = tools;
        }

        // ── Record story events on phase transitions ───────────────────────────
        if human_state.phase != prev_human_phase {
            let story = phase_transition_story(
                &player_name,
                &prev_human_phase,
                &human_state.phase,
                &human_state.description,
            );
            event_log.push(story);
            prev_human_phase = human_state.phase.clone();
        }
        for (i, (state, prev)) in agent_states
            .iter()
            .zip(prev_agent_phases.iter_mut())
            .enumerate()
        {
            if state.phase != *prev {
                let name = agent_slots
                    .get(i)
                    .map(|s| s.name.as_str())
                    .unwrap_or("Agent");
                let story = phase_transition_story(name, prev, &state.phase, &state.description);
                event_log.push(story);
                *prev = state.phase.clone();
            }
        }

        // ── Render ────────────────────────────────────────────────────────────
        let num_agents = agent_slots.len();
        terminal.draw(|f| {
            let area = f.area();

            let outer = Block::default()
                .title(format!(
                    " 🎰 Blackjack — {} | Bankroll: ${} ",
                    player_name, human_state.bankroll
                ))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow));
            let inner = outer.inner(area);
            outer.render(area, f.buffer_mut());

            // ── Horizontal split: players | typestate graph ───────────────────
            // Typestate graph always gets its minimum guaranteed width so labels
            // are never truncated; the player area fills the remaining space.
            let min_ts = min_typestate_width(&bj_nodes);
            let h_constraints: Vec<Constraint> = if show_typestate_graph {
                vec![Constraint::Min(0), Constraint::Min(min_ts)]
            } else {
                vec![Constraint::Min(0)]
            };
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(h_constraints)
                .split(inner);

            let players_area = h_chunks[0];

            // ── Player area: human left, agent grid right ─────────────────────
            // With 0 agents: human takes full width.
            // With N agents: split horizontally into human | agents_grid.
            // The agents_grid stacks agents into rows-×-cols to minimise the
            // horizontal real-estate consumed (avoids squeezing the graph).
            let (human_area, agent_grid_area) = if num_agents == 0 {
                (players_area, None)
            } else {
                // Give agents roughly half the player width, human the other half.
                let p_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(players_area);
                (p_chunks[0], Some(p_chunks[1]))
            };

            // ── Human panel ───────────────────────────────────────────────────
            let human_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(human_area);

            let human_block = Block::default()
                .title(format!(" {} — {} ", player_name, human_state.phase))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Green));
            Paragraph::new(human_state.description.as_str())
                .block(human_block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .render(human_chunks[0], f.buffer_mut());

            let hints = if available_tools.is_empty() {
                "Waiting for your turn...  [q] Quit".to_string()
            } else {
                let mut lines = vec!["Your choices:".to_string()];
                for (i, t) in available_tools.iter().enumerate() {
                    let key = if i < 9 {
                        format!("[{}]", i + 1)
                    } else {
                        format!("[{}]", (b'a' + (i as u8 - 9)) as char)
                    };
                    lines.push(format!("  {} {}", key, t.description));
                }
                lines.push(String::new());
                lines.push("[q] Quit".to_string());
                lines.join("\n")
            };
            let ctrl_block = Block::default()
                .title(" Controls ")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White));
            Paragraph::new(hints.as_str())
                .block(ctrl_block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .render(human_chunks[1], f.buffer_mut());

            // ── Agent grid ────────────────────────────────────────────────────
            // Pack agents into a rows × cols grid inside `agent_grid_area`.
            // Grid shape: prefer square-ish; columns first so horizontal real
            // estate is controlled.  With 1 agent → 1×1; 2 → 2×1; 3-4 → 2×2.
            if let (Some(grid_area), true) = (agent_grid_area, num_agents > 0) {
                let grid_cols = if num_agents <= 2 { 1usize } else { 2 };
                let grid_rows = num_agents.div_ceil(grid_cols);

                // Split grid_area into `grid_cols` horizontal slices.
                let col_areas: Vec<_> = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(
                        std::iter::repeat_n(Constraint::Ratio(1, grid_cols as u32), grid_cols)
                            .collect::<Vec<_>>(),
                    )
                    .split(grid_area)
                    .to_vec();

                // Within each column, split vertically into rows.
                for (col_idx, col_area) in col_areas.iter().enumerate() {
                    let agents_before = col_idx * grid_rows;
                    let rows_in_col = grid_rows.min(num_agents.saturating_sub(agents_before));
                    if rows_in_col == 0 {
                        continue;
                    }

                    let row_areas: Vec<_> = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(
                            std::iter::repeat_n(
                                Constraint::Ratio(1, rows_in_col as u32),
                                rows_in_col,
                            )
                            .collect::<Vec<_>>(),
                        )
                        .split(*col_area)
                        .to_vec();

                    for (row_idx, cell) in row_areas.iter().enumerate() {
                        let agent_idx = agents_before + row_idx;
                        if agent_idx >= num_agents {
                            break;
                        }

                        let slot = &agent_slots[agent_idx];
                        let state = &agent_states[agent_idx];

                        let agent_block = Block::default()
                            .title(format!(" {} — {} ", slot.name, state.phase))
                            .borders(Borders::ALL)
                            .style(Style::default().fg(Color::Cyan));
                        Paragraph::new(state.description.as_str())
                            .block(agent_block)
                            .wrap(ratatui::widgets::Wrap { trim: false })
                            .render(*cell, f.buffer_mut());
                    }
                }
            }

            // ── Typestate graph + chat ────────────────────────────────────────
            if show_typestate_graph && h_chunks.len() > 1 {
                let ts_area = h_chunks[1];
                let active_idx = blackjack_active(&human_state.phase);
                let _ts_proof = verify_typestate_readable(&bj_nodes, ts_area).unwrap_or_else(|e| {
                    render_resize_prompt(f, &e);
                    elicitation::contracts::Established::assert()
                });

                // Build the combined agent chat messages once.
                let chat_messages: Vec<ChatMessage> = agent_dialogues
                    .iter()
                    .zip(agent_slots.iter())
                    .flat_map(|(dialogue, s)| {
                        dialogue.iter().map(move |e| {
                            let participant = if e.role == "Agent" {
                                Participant::Agent(s.name.clone())
                            } else {
                                Participant::Host
                            };
                            ChatMessage::new(participant, e.text.clone())
                        })
                    })
                    .collect();

                if chat_messages.is_empty() {
                    // No agents yet — give the whole column to the graph.
                    TypestateGraphWidget::new(&bj_nodes, &bj_edges, active_idx, &event_log)
                        .render(ts_area, f.buffer_mut());
                } else {
                    // Split the right column: typestate graph on top, chat below.
                    let ts_chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                        .split(ts_area);
                    TypestateGraphWidget::new(&bj_nodes, &bj_edges, active_idx, &event_log)
                        .render(ts_chunks[0], f.buffer_mut());
                    let (chat_widget, _chat_proof) = ChatWidget::new(&chat_messages);
                    chat_widget.render(ts_chunks[1], f.buffer_mut());
                }
            }
        })?;

        // ── Handle input ──────────────────────────────────────────────────────
        if crossterm::event::poll(std::time::Duration::ZERO)?
            && let Event::Key(k) = crossterm::event::read()?
        {
            match k.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    return Ok(BlackjackSessionOutcome::Abandoned);
                }
                KeyCode::Char(c) => {
                    let idx = if c.is_ascii_digit() && c != '0' {
                        Some((c as usize) - ('1' as usize))
                    } else if c.is_ascii_lowercase() {
                        let letter_idx = (c as usize) - ('a' as usize);
                        Some(letter_idx + 9)
                    } else {
                        None
                    };
                    if let Some(Some(tool)) = idx.map(|i| available_tools.get(i)) {
                        let name = tool.name.clone();
                        let args = if name.ends_with("__place") {
                            use crate::tui::tui_communicator::TuiCommunicator;
                            use elicitation::ElicitCommunicator as _;
                            let comm = TuiCommunicator::new();
                            let styled = comm.with_style::<u64, BlackjackBetStyle>(
                                BlackjackBetStyle::new(1, human_state.bankroll),
                            );
                            if let Ok(raw) = u64::elicit(&styled).await {
                                let amount = raw.min(human_state.bankroll).max(1);
                                serde_json::json!({ "amount": amount })
                            } else {
                                serde_json::Value::Null
                            }
                        } else {
                            serde_json::json!({})
                        };
                        if args != serde_json::Value::Null {
                            let _ = human.call_tool(&name, args).await;
                            available_tools.clear();
                        }
                    }
                }
                _ => {}
            }
        }

        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

// ─────────────────────────────────────────────────────────────
//  Story event helpers
// ─────────────────────────────────────────────────────────────

/// Build a [`GameEvent`] that narrates a phase transition for `player`.
fn phase_transition_story(player: &str, from: &str, to: &str, description: &str) -> GameEvent {
    match to {
        "betting" if from == "idle" || from == "finished" => {
            GameEvent::story(format!("🃏  {player} — ready to bet"))
        }
        "player_turn" => {
            // Grab the first line of the description (hand + dealer card).
            let summary = description.lines().next().unwrap_or("cards dealt");
            GameEvent::story(format!("🎴  {player} — {summary}"))
        }
        "waiting" => {
            GameEvent::phase_change(&format!("{player}: {from}"), "waiting for other seats")
        }
        "finished" => {
            // Pull the outcome line from description (first non-empty line after hand).
            let outcome = description
                .lines()
                .find(|l| l.contains('$') || l.contains("Push") || l.contains("Surrender"))
                .unwrap_or("hand settled");
            GameEvent::result(format!("🏁  {player} — {outcome}"))
        }
        other => GameEvent::phase_change(from, other),
    }
}

// ─────────────────────────────────────────────────────────────
//  Bet style for custom-amount elicitation
// ─────────────────────────────────────────────────────────────

/// Custom [`ElicitationStyle`] that prompts the human player for a bet amount.
#[derive(Clone, Debug, Default)]
struct BlackjackBetStyle {
    min: u64,
    max: u64,
}

impl BlackjackBetStyle {
    fn new(min: u64, max: u64) -> Self {
        Self { min, max }
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
            "Place your custom bet ({}-{} chips). Enter amount:",
            self.min, self.max
        )
    }
}
