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
    terminal::{Clear, ClearType},
};
use elicitation::{
    ElicitCommunicator, ElicitError, ElicitErrorKind, ElicitResult, ElicitationContext,
    ElicitationStyle, StyleContext,
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
    /// Print `prompt` to the terminal and block until the player presses Enter.
    ///
    /// The prompt text is rendered with cyan highlighting. The player's input is
    /// echoed in place and returned verbatim (trimmed of leading/trailing whitespace).
    #[instrument(skip(self), level = "debug", fields(prompt_len = prompt.len()))]
    fn send_prompt(
        &self,
        prompt: &str,
    ) -> impl std::future::Future<Output = ElicitResult<String>> + Send {
        let prompt = prompt.to_string();
        async move {
            let mut stdout = io::stdout();
            let mut input = String::new();

            // In raw mode, \n alone moves down but doesn't return to column 0.
            // Normalise all bare \n to \r\n so multi-line prompts stay flush-left.
            let prompt = prompt.replace('\n', "\r\n");

            execute!(
                stdout,
                Print("\r\n"),
                SetForegroundColor(Color::Cyan),
                Print(&prompt),
                ResetColor,
                Print("\r\n▶ "),
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
                        KeyCode::Enter => {
                            execute!(stdout, Print("\r\n")).ok();
                            break;
                        }
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
                            // Erase the last character in place.
                            execute!(stdout, Print("\x08 \x08")).ok();
                            stdout.flush().ok();
                        }
                        _ => {}
                    }
                }
            }

            let result = input.trim().to_string();
            tracing::debug!(response = %result, "TUI elicitation response received");

            // Clear everything printed by this prompt so the next ratatui
            // draw() gets the full terminal back without accumulated scroll.
            execute!(stdout, Clear(ClearType::All), MoveTo(0, 0)).ok();

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

    fn with_style<T: 'static, S: ElicitationStyle>(&self, style: S) -> Self {
        let mut new = self.clone();
        new.style_ctx.set_style::<T, S>(style).ok();
        new
    }
}
