//! Blackjack TUI game loop — human + MCP agent mode.
//!
//! Spawns the HTTP game server and an agent subprocess.  The human player
//! drives their own blackjack hand via keyboard (calling MCP tools directly),
//! while the agent plays its own hand concurrently via its subprocess.
//! The TUI shows both hands and the agent's chat log.

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

/// Run a blackjack session with the human playing via keyboard and an agent
/// playing concurrently via MCP.
///
/// Both players have independent sessions on the same HTTP server.
/// The human calls tools directly (keyboard → JSON-RPC).
/// The agent drives its own session via the spawned subprocess.
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
    use crate::games::blackjack::BlackjackStateView;
    use crate::session::DialogueEntry;
    use crate::tui::chat_widget::ChatWidget;
    use crate::tui::rest_client::{BlackjackObserver, BlackjackTool, HumanBlackjackClient};
    use crate::tui::standalone::{GameMode, ProcessGuards, spawn_agent, spawn_server};
    use crate::tui::typestate_widget::{TypestateGraphWidget, blackjack_active};
    use crate::tui::{ChatMessage, Participant};
    use crossterm::event::{Event, KeyCode};
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::prelude::Widget;
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    const HUMAN_SESSION: &str = "human_bj";
    const AGENT_SESSION: &str = "agent_bj";

    info!("Starting human+agent blackjack MCP session");
    let server_url = format!("http://localhost:{}", port);

    // ── Spawn infrastructure ──────────────────────────────────────────────────
    let server = spawn_server(port).await?;
    let agent = spawn_agent(
        port,
        agent_config_path,
        GameMode::Blackjack {
            bankroll: initial_bankroll,
            session_id: AGENT_SESSION.to_string(),
        },
    )
    .await?;
    let _guards = ProcessGuards::new(server, agent);

    // ── Connect human MCP session ─────────────────────────────────────────────
    let human = HumanBlackjackClient::connect(&server_url).await?;

    // Start the human's first hand
    human
        .call_tool(
            "blackjack_deal",
            serde_json::json!({
                "initial_bankroll": initial_bankroll,
                "session_id": HUMAN_SESSION
            }),
        )
        .await?;

    let agent_observer = BlackjackObserver::new(server_url, AGENT_SESSION.to_string());
    let human_observer = BlackjackObserver::new(
        format!("http://localhost:{}", port),
        HUMAN_SESSION.to_string(),
    );

    let bj_nodes = blackjack_nodes();
    let bj_edges = blackjack_edges();

    let mut agent_dialogue: Vec<DialogueEntry> = Vec::new();
    let mut available_tools: Vec<BlackjackTool> = Vec::new();
    let mut tool_refresh_counter: u8 = 0;

    loop {
        // ── Poll state ────────────────────────────────────────────────────────
        let idle_state = BlackjackStateView {
            phase: "idle".to_string(),
            bankroll: 0,
            description: "Connecting...".to_string(),
            is_terminal: false,
        };
        let human_state = human_observer
            .get_blackjack_state()
            .await
            .unwrap_or_else(|_| idle_state.clone());
        let agent_state = agent_observer
            .get_blackjack_state()
            .await
            .unwrap_or(idle_state);

        if let Ok(entries) = agent_observer.get_dialogue().await {
            agent_dialogue = entries;
        }

        // Refresh available tools every ~500ms (every 2 loop iterations)
        tool_refresh_counter = tool_refresh_counter.wrapping_add(1);
        if tool_refresh_counter.is_multiple_of(2)
            && let Ok(tools) = human.list_blackjack_tools().await
        {
            available_tools = tools;
        }
        let agent_messages: Vec<ChatMessage> = agent_dialogue
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

        // ── Render ────────────────────────────────────────────────────────────
        terminal.draw(|f| {
            let area = f.area();

            let outer = Block::default()
                .title(format!(
                    " 🎰 Blackjack — {} vs Agent | Your bankroll: ${} ",
                    player_name, human_state.bankroll
                ))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow));
            let inner = outer.inner(area);
            outer.render(area, f.buffer_mut());

            // Split horizontally: left=human+controls, right=agent+graph
            let h_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(if show_typestate_graph {
                    vec![
                        Constraint::Percentage(35),
                        Constraint::Percentage(30),
                        Constraint::Percentage(35),
                    ]
                } else {
                    vec![Constraint::Percentage(45), Constraint::Percentage(55)]
                })
                .split(inner);

            let human_area = h_chunks[0];
            let agent_area = h_chunks[1];
            let ts_area = if show_typestate_graph {
                Some(h_chunks[2])
            } else {
                None
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

            // Controls pane — show available tools as keyboard shortcuts
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

            // ── Agent panel ───────────────────────────────────────────────────
            let agent_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(agent_area);

            let agent_block = Block::default()
                .title(format!(" Agent — {} ", agent_state.phase))
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Cyan));
            Paragraph::new(agent_state.description.as_str())
                .block(agent_block)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .render(agent_chunks[0], f.buffer_mut());

            let (chat_widget, _proof) = ChatWidget::new(&agent_messages);
            chat_widget.render(agent_chunks[1], f.buffer_mut());

            // ── Typestate graph ───────────────────────────────────────────────
            if let Some(ts) = ts_area {
                let active_idx = blackjack_active(&human_state.phase);
                TypestateGraphWidget::new(&bj_nodes, &bj_edges, active_idx, &[])
                    .render(ts, f.buffer_mut());
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
                // Number or letter keys map to available tools by index
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
                            // Use TuiCommunicator + custom style to prompt for bet amount.
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
                            // Force immediate tool refresh after action
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
