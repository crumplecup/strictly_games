//! Type-safe HTTP client using REST API.

use crate::TicTacToePlayer as Player;
use crate::games::tictactoe::{AnyGame, Position};
use anyhow::{Context, Result};
use tracing::{debug, info, instrument};

/// Type-safe HTTP game client.
#[derive(Debug, Clone)]
pub struct RestGameClient {
    base_url: String,
    client: reqwest::Client,
    pub session_id: String,
    pub player_id: String,
    pub player_mark: Player,
    pub last_error: Option<String>, // Track last error for display
    mcp_session_id: String,         // For MCP tool calls
}

impl RestGameClient {
    /// Creates a new REST client by registering with the server via MCP.
    #[instrument(skip_all, fields(base_url = %base_url, session_id = %session_id, name = %name))]
    pub async fn register(base_url: String, session_id: String, name: String) -> Result<Self> {
        info!("Registering with server");

        let client = reqwest::Client::new();

        // Register via MCP (keep this for player setup)
        let (player_id, mcp_session_id, player_mark) =
            Self::mcp_register(&client, &base_url, &session_id, &name).await?;

        Ok(Self {
            base_url,
            client,
            session_id,
            player_id,
            player_mark,
            last_error: None,
            mcp_session_id,
        })
    }

    /// MCP registration (creates player association).
    async fn mcp_register(
        client: &reqwest::Client,
        base_url: &str,
        session_id: &str,
        name: &str,
    ) -> Result<(String, String, Player)> {
        // Initialize MCP session
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "strictly-games-tui",
                    "version": "0.1.0"
                }
            }
        });

        let response = client
            .post(format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&init_req)
            .send()
            .await?;

        let mcp_session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
            .context("No MCP session ID in response headers")?
            .to_string();

        debug!(mcp_session_id = %mcp_session_id, "MCP session initialized");

        // Send initialized notification
        let init_notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });

        client
            .post(format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &mcp_session_id)
            .json(&init_notif)
            .send()
            .await?;

        // Register player
        let register_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "register_player",
                "arguments": {
                    "session_id": session_id,
                    "name": name,
                    "type": "human"
                }
            }
        });

        let response = client
            .post(format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &mcp_session_id)
            .json(&register_req)
            .send()
            .await?;

        let response_text = response.text().await?;
        debug!(response = %response_text, "Register player response");

        // Check for errors in response
        if response_text.contains("\"error\"") {
            return Err(anyhow::anyhow!("Registration failed: {}", response_text));
        }

        // Parse which mark the server assigned from the response text.
        // The server returns "Registered as player X!" or "Registered as player O!"
        let player_mark = if response_text.contains("player X!") {
            Player::X
        } else {
            Player::O
        };

        let player_id = format!("{}_{}", session_id, name.to_lowercase());
        info!(player_id = %player_id, mark = ?player_mark, "Registered successfully");

        Ok((player_id, mcp_session_id, player_mark))
    }

    /// Gets the current game state (type-safe!).
    #[instrument(skip(self))]
    pub async fn get_game(&self) -> Result<AnyGame> {
        debug!("Getting game state via REST");

        let url = format!("{}/api/sessions/{}/game", self.base_url, self.session_id);
        let game: AnyGame = self.client.get(&url).send().await?.json().await?;

        debug!(is_over = game.is_over(), "Got game state");
        Ok(game)
    }

    /// Fetches the current explore/play stats from the server.
    #[instrument(skip(self))]
    pub async fn get_explore_stats(&self) -> Result<crate::session::ExploreStats> {
        let url = format!(
            "{}/api/sessions/{}/explore_stats",
            self.base_url, self.session_id
        );
        let stats = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Failed to deserialize explore stats")?;
        Ok(stats)
    }

    /// Fetches the server↔agent dialogue log.
    #[instrument(skip(self))]
    pub async fn get_dialogue(&self) -> Result<Vec<crate::session::DialogueEntry>> {
        let url = format!(
            "{}/api/sessions/{}/dialogue",
            self.base_url, self.session_id
        );
        let entries = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Failed to deserialize dialogue")?;
        Ok(entries)
    }

    /// Makes a move via MCP tool.
    #[instrument(skip(self), fields(position = ?position))]
    pub async fn make_move(&mut self, position: Position) -> Result<()> {
        info!("Making move");
        self.last_error = None; // Clear previous error

        // Serialize Position properly using serde
        let position_value = serde_json::to_value(position)?;
        debug!(position_json = %position_value, "Serialized position");

        // Use MCP tool for making moves (triggers elicitation)
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "make_move",
                "arguments": {
                    "session_id": self.session_id,
                    "player_id": self.player_id,
                    "position": position_value
                }
            }
        });

        debug!(request = %request, "Sending MCP tool call");

        let response = self
            .client
            .post(format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.mcp_session_id)
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;
        debug!(status = %status, body = %body, "Got MCP response");

        // Check for error in JSON-RPC response
        if body.contains("\"error\"")
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(error_msg) = json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
        {
            self.last_error = Some(error_msg.to_string());
            anyhow::bail!("Move failed: {}", error_msg);
        }

        if !status.is_success() {
            self.last_error = Some(format!("HTTP {}", status));
            anyhow::bail!("Move failed: {} - {}", status, body);
        }

        Ok(())
    }

    /// Restarts current game (keeps players registered).
    #[instrument(skip(self))]
    pub async fn restart_game(&mut self) -> Result<()> {
        info!("Restarting game");
        self.last_error = None;

        let response = self
            .client
            .post(format!(
                "{}/api/sessions/{}/restart",
                self.base_url, self.session_id
            ))
            .send()
            .await?;

        if !response.status().is_success() {
            self.last_error = Some("Restart failed".to_string());
            anyhow::bail!("Restart failed: {}", response.status());
        }

        Ok(())
    }

    /// Cancels the current game (triggers passive-Affirm escape hatch).
    #[instrument(skip(self))]
    pub async fn cancel_game(&mut self) -> Result<()> {
        info!("Cancelling game via MCP tool");
        self.last_error = None;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "cancel_game",
                "arguments": {
                    "session_id": self.session_id
                }
            }
        });

        let response = self
            .client
            .post(format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.mcp_session_id)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            self.last_error = Some("Cancel failed".to_string());
            anyhow::bail!("Cancel failed: {}", response.status());
        }

        Ok(())
    }
}

// ── Blackjack observer (no player registration needed) ───────────────────────

/// Lightweight HTTP observer for a blackjack agent session.
///
/// The TUI spectator loop uses this to poll game state and dialogue without
/// registering as a player — the agent drives the game entirely via MCP.
#[derive(Debug, Clone)]
pub struct BlackjackObserver {
    base_url: String,
    client: reqwest::Client,
    pub session_id: String,
}

impl BlackjackObserver {
    /// Creates a new observer pointing at `base_url` for `session_id`.
    pub fn new(base_url: String, session_id: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
            session_id,
        }
    }

    /// Fetches the current blackjack phase state.
    #[instrument(skip(self))]
    pub async fn get_blackjack_state(&self) -> Result<crate::session::SharedTableSeatView> {
        let url = format!(
            "{}/api/sessions/{}/blackjack_state",
            self.base_url, self.session_id
        );
        let view = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Failed to deserialize blackjack state")?;
        Ok(view)
    }

    /// Fetches the server↔agent dialogue log.
    #[instrument(skip(self))]
    pub async fn get_dialogue(&self) -> Result<Vec<crate::session::DialogueEntry>> {
        let url = format!(
            "{}/api/sessions/{}/dialogue",
            self.base_url, self.session_id
        );
        let entries = self
            .client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Failed to deserialize dialogue")?;
        Ok(entries)
    }
}

// ── Human blackjack MCP client ───────────────────────────────────────────────

/// A tool available in the current blackjack phase.
#[derive(Debug, Clone)]
pub struct BlackjackTool {
    /// Tool name (e.g. `bet__preset_50`, `blackjack__hit`).
    pub name: String,
    /// Human-readable description.
    pub description: String,
}

/// Extracts JSON from a Streamable HTTP MCP response body.
///
/// The server may respond with raw JSON or with an SSE-wrapped body like:
/// `data: {"jsonrpc":"2.0","id":1,"result":{...}}\n\n`.
/// This strips the SSE framing and returns the first parseable JSON object.
fn parse_mcp_response(body: &str) -> Result<serde_json::Value> {
    // Try direct JSON first.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        return Ok(v);
    }
    // Strip SSE framing: find the first `data: ` line and parse its content.
    for line in body.lines() {
        let json_str = if let Some(stripped) = line.strip_prefix("data: ") {
            stripped.trim()
        } else {
            line.trim()
        };
        if json_str.starts_with('{')
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str)
        {
            return Ok(v);
        }
    }
    anyhow::bail!("No parseable JSON found in MCP response: {body}")
}

/// MCP client for a human player driving blackjack via keyboard.
///
/// Holds an initialized MCP session ID and sends JSON-RPC `tools/call`
/// requests on behalf of the human player.
#[derive(Debug, Clone)]
pub struct HumanBlackjackClient {
    base_url: String,
    mcp_session_id: String,
    client: reqwest::Client,
    /// Monotonic request id counter.
    next_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl HumanBlackjackClient {
    /// Initializes a new MCP session with the server.
    ///
    /// Does NOT register as a game player — just establishes the session so
    /// the human can call blackjack tools directly.
    #[instrument(skip_all)]
    pub async fn connect(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into();
        let client = reqwest::Client::new();

        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "strictly-games-human", "version": "0.1.0" }
            }
        });

        let response = client
            .post(format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&init_req)
            .send()
            .await?;

        let mcp_session_id = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
            .context("No MCP session ID in server response")?
            .to_string();

        // Send `notifications/initialized`
        client
            .post(format!("{}/message", base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &mcp_session_id)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .send()
            .await?;

        info!(mcp_session_id = %mcp_session_id, "Human MCP session established");

        Ok(Self {
            base_url,
            mcp_session_id,
            client,
            next_id: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(10)),
        })
    }

    fn next_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Calls a tool by name with the given arguments.
    ///
    /// Returns the tool result text on success.
    #[instrument(skip(self, args), fields(tool_name = %name))]
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<String> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": args
            }
        });

        let response = self
            .client
            .post(format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.mcp_session_id)
            .json(&request)
            .send()
            .await?
            .text()
            .await?;

        Ok(response)
    }

    /// Lists tools currently registered for this MCP session.
    ///
    /// Filters to tools relevant to blackjack (prefix `bet__`, `blackjack__`,
    /// `next__`).
    #[instrument(skip(self))]
    pub async fn list_blackjack_tools(&self) -> Result<Vec<BlackjackTool>> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list",
            "params": {}
        });

        let response_text = self
            .client
            .post(format!("{}/message", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", &self.mcp_session_id)
            .json(&request)
            .send()
            .await?
            .text()
            .await?;

        let parsed: serde_json::Value =
            parse_mcp_response(&response_text).context("Failed to parse tools/list response")?;

        let tools = parsed["result"]["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|t| {
                let name = t["name"].as_str()?.to_string();
                if name.starts_with("bet__")
                    || name.starts_with("blackjack__")
                    || name.starts_with("next__")
                {
                    let description = t["description"].as_str().unwrap_or(&name).to_string();
                    Some(BlackjackTool { name, description })
                } else {
                    None
                }
            })
            .collect();

        Ok(tools)
    }
}
