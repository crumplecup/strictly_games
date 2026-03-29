//! TUI-based elicitation communicator for in-game player input.
//!
//! [`TuiCommunicator`] implements [`ElicitCommunicator`] by rendering prompts
//! directly to the terminal via crossterm and reading player keystrokes
//! in the existing raw mode context established by the ratatui game loop.

use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use elicitation::{
    ElicitCommunicator, ElicitError, ElicitErrorKind, ElicitResult, ElicitationContext,
    StyleContext, StyleMarker,
};
use std::io::{self, Write as _};
use tracing::instrument;

/// An [`ElicitCommunicator`] that drives player input through the ratatui terminal.
///
/// Prompts are printed below the current ratatui frame using crossterm. The player
/// responds by typing in raw mode; input is echoed character by character and
/// submitted with Enter. Both human players and future AI observers can share this
/// communicator — the interface is identical regardless of who is on the other end.
///
/// # Raw Mode Contract
///
/// This communicator assumes the terminal is already in raw mode (as established
/// by the ratatui game loop). It does not call `enable_raw_mode` or
/// `disable_raw_mode` itself.
///
/// # `call_tool`
///
/// MCP tool calls are not supported in this context. `call_tool` returns
/// `ServiceError::Cancelled`. Game types that only use `send_prompt`
/// (enums and structs derived with `#[derive(Elicit)]`, integers, booleans)
/// work correctly. Primitives that use raw MCP tool calls (uuid, char, http
/// types) should not be elicited through this communicator.
#[derive(Clone)]
pub struct TuiCommunicator {
    style_ctx: StyleContext,
    elicit_ctx: ElicitationContext,
}

impl TuiCommunicator {
    /// Creates a new communicator with empty style and elicitation contexts.
    #[instrument(level = "debug")]
    pub fn new() -> Self {
        Self {
            style_ctx: StyleContext::default(),
            elicit_ctx: ElicitationContext::default(),
        }
    }
}

impl Default for TuiCommunicator {
    fn default() -> Self {
        Self::new()
    }
}

impl ElicitCommunicator for TuiCommunicator {
    /// Print `prompt` inside the dedicated prompt pane and block until Enter.
    ///
    /// The ratatui layout reserves [`PROMPT_PANE_HEIGHT`] rows at the bottom
    /// of the screen with a bordered " Input " block. This method draws the
    /// prompt text and `▶` input cursor inside that border, using [`MoveTo`]
    /// so the game content above is never scrolled.
    ///
    /// [`PROMPT_PANE_HEIGHT`]: crate::tui::blackjack::PROMPT_PANE_HEIGHT
    #[instrument(skip(self), level = "debug", fields(prompt_len = prompt.len()))]
    fn send_prompt(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = ElicitResult<String>> + Send {
        let prompt = prompt.to_string();
        async move {
            use crate::tui::blackjack::PROMPT_PANE_HEIGHT;

            let mut stdout = io::stdout();
            let mut input = String::new();

            // Drain any stale key events so previous input doesn't leak in.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            while event::poll(std::time::Duration::ZERO).unwrap_or(false) {
                let _ = event::read();
            }

            let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));

            // The pane occupies the bottom PROMPT_PANE_HEIGHT rows with a
            // 1-cell border on each side. Content area starts 1 row below
            // the pane top and is inset 1 column on each side.
            let pane_top = rows.saturating_sub(PROMPT_PANE_HEIGHT);
            let content_top = pane_top + 1; // skip top border
            let content_left: u16 = 2; // skip left border + 1 padding
            let content_width = cols.saturating_sub(4) as usize; // borders + padding
            // Content rows available: pane height - 2 (borders) - 1 (input row).
            let max_prompt_lines = PROMPT_PANE_HEIGHT.saturating_sub(3) as usize;
            let input_row = rows.saturating_sub(2); // last content row (above bottom border)

            // Wrap prompt text into lines that fit the content width.
            let mut wrapped: Vec<String> = Vec::new();
            for line in prompt.lines() {
                if line.is_empty() {
                    wrapped.push(String::new());
                } else if line.len() <= content_width {
                    wrapped.push(line.to_string());
                } else {
                    let mut remaining = line;
                    while !remaining.is_empty() {
                        let end = remaining
                            .char_indices()
                            .take_while(|(i, _)| *i < content_width)
                            .last()
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap_or(remaining.len());
                        wrapped.push(remaining[..end].to_string());
                        remaining = &remaining[end..];
                    }
                }
            }
            // Truncate to fit the available content rows.
            if wrapped.len() > max_prompt_lines {
                wrapped = wrapped.split_off(wrapped.len() - max_prompt_lines);
            }

            // Clear the interior of the pane (preserve the ratatui border).
            for row in content_top..rows.saturating_sub(1) {
                execute!(
                    stdout,
                    MoveTo(1, row),
                    Print(" ".repeat(cols.saturating_sub(2) as usize)),
                )
                .ok();
            }

            // Draw prompt lines inside the pane.
            for (i, line) in wrapped.iter().enumerate() {
                execute!(
                    stdout,
                    MoveTo(content_left, content_top + i as u16),
                    SetForegroundColor(Color::Cyan),
                    Print(line),
                    ResetColor,
                )
                .ok();
            }

            // Input cursor on the last content row.
            execute!(
                stdout,
                MoveTo(content_left, input_row),
                SetForegroundColor(Color::Green),
                Print("▶ "),
                ResetColor,
            )
            .map_err(|e| {
                ElicitError::new(ElicitErrorKind::ParseError(format!(
                    "Terminal write error: {e}"
                )))
            })?;
            stdout.flush().map_err(|e| {
                ElicitError::new(ElicitErrorKind::ParseError(format!(
                    "Terminal flush error: {e}"
                )))
            })?;

            // Read input character by character (terminal is in raw mode).
            loop {
                if let Event::Key(key) = event::read().map_err(|e| {
                    ElicitError::new(ElicitErrorKind::ParseError(format!(
                        "Event read error: {e}"
                    )))
                })? {
                    match key.code {
                        KeyCode::Enter => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            tracing::warn!("Player cancelled TUI elicitation via Ctrl-C");
                            return Err(ElicitError::new(ElicitErrorKind::ParseError(
                                "Cancelled by player".to_string(),
                            )));
                        }
                        KeyCode::Char(c) => {
                            input.push(c);
                            execute!(stdout, Print(c)).ok();
                            stdout.flush().ok();
                        }
                        KeyCode::Backspace if !input.is_empty() => {
                            input.pop();
                            execute!(stdout, Print("\x08 \x08")).ok();
                            stdout.flush().ok();
                        }
                        _ => {}
                    }
                }
            }

            let result = input.trim().to_string();
            tracing::debug!(response = %result, "TUI elicitation response received");

            Ok(result)
        }
    }

    /// Not supported — returns `ServiceError::Cancelled`.
    ///
    /// MCP tool calls require an active MCP transport. Use [`ElicitServer`] or
    /// [`ElicitClient`] for communicators that support this method.
    ///
    /// [`ElicitServer`]: elicitation::ElicitServer
    /// [`ElicitClient`]: elicitation::ElicitClient
    #[instrument(skip(self, _params), level = "debug")]
    fn call_tool(
        &self,
        _params: rmcp::model::CallToolRequestParams,
    ) -> impl std::future::Future<
        Output = Result<rmcp::model::CallToolResult, rmcp::service::ServiceError>,
    > + Send {
        async {
            tracing::warn!("call_tool invoked on TuiCommunicator — not supported");
            Err(rmcp::service::ServiceError::Cancelled {
                reason: Some("TUI context does not support MCP tool calls".to_string()),
            })
        }
    }

    fn style_context(&self) -> &StyleContext {
        &self.style_ctx
    }

    fn elicitation_context(&self) -> &ElicitationContext {
        &self.elicit_ctx
    }

    fn with_style<T: 'static, S: StyleMarker + elicitation::style::ElicitationStyle + 'static>(
        &self,
        style: S,
    ) -> Self {
        let mut new = self.clone();
        new.style_ctx.set_style::<T, S>(style).ok();
        new
    }
}
