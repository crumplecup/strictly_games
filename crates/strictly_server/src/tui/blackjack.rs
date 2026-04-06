//! Blackjack TUI game loop — MCP spectator mode.
//!
//! Spawns the HTTP game server and an agent subprocess, then polls REST
//! endpoints to render a live spectator view of the agent's play.

use crate::tui::typestate_widget::{blackjack_edges, blackjack_nodes};
use anyhow::Result;
use ratatui::{Terminal, backend::Backend};
use tokio::time::Duration;
use tracing::{info, instrument};

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
