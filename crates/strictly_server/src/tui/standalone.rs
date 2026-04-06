//! Standalone mode subprocess management.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::process::{Child, Command};
use tokio::time::{Duration, sleep, timeout};
use tracing::{debug, info, instrument};

/// Which game the agent subprocess should play.
#[derive(Debug, Clone)]
pub enum GameMode {
    /// TicTacToe with `--test-play`.
    TicTacToe,
    /// Blackjack with `--test-blackjack`, a bankroll, and a known session_id.
    Blackjack { bankroll: u64, session_id: String },
}

/// Guards for spawned subprocesses. Kills processes on drop.
pub struct ProcessGuards {
    server: Option<Child>,
    agents: Vec<Child>,
}

impl ProcessGuards {
    /// Guard a server and a single agent subprocess.
    pub fn new(server: Child, agent: Child) -> Self {
        Self {
            server: Some(server),
            agents: vec![agent],
        }
    }

    /// Guard a server and zero or more agent subprocesses.
    pub fn many(server: Child, agents: Vec<Child>) -> Self {
        Self {
            server: Some(server),
            agents,
        }
    }
}

impl Drop for ProcessGuards {
    fn drop(&mut self) {
        info!("Cleaning up spawned subprocesses");

        for agent in &mut self.agents {
            debug!("Killing agent process");
            let _ = agent.start_kill();
        }

        if let Some(mut server) = self.server.take() {
            debug!("Killing server process");
            let _ = server.start_kill();
        }
    }
}

/// Spawns the HTTP game server and waits until it is ready.
///
/// Returns the server [`Child`] process. The caller is responsible for keeping
/// it alive (typically via [`ProcessGuards`]).
#[instrument(fields(port))]
pub async fn spawn_server(port: u16) -> Result<Child> {
    let exe = std::env::current_exe().context("Failed to get current executable path")?;

    info!(port, "Spawning HTTP game server");
    debug!(exe = %exe.display(), "Server executable path");

    let mut server = Command::new(&exe)
        .arg("http")
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .stdout(std::process::Stdio::piped()) // Capture stdout to see errors
        .stderr(std::process::Stdio::piped()) // Capture stderr
        .spawn()
        .context("Failed to spawn server process")?;

    debug!(pid = ?server.id(), "Server process spawned, waiting for readiness");

    // Give server a moment to start and fail if it's going to
    sleep(Duration::from_millis(100)).await;

    // Check if server already exited
    if let Ok(Some(status)) = server.try_wait() {
        // Server exited immediately - capture output
        let output = server.wait_with_output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Server exited immediately with status {:?}\nstdout: {}\nstderr: {}",
            status,
            stdout,
            stderr
        );
    }

    let server_url = format!("http://localhost:{}", port);
    wait_for_server_ready(&server_url)
        .await
        .context("Server failed to become ready")?;

    info!("Server is ready");
    Ok(server)
}

/// Spawns the agent subprocess and gives it a moment to connect.
///
/// Returns the agent [`Child`] process. The caller is responsible for keeping
/// it alive (typically via [`ProcessGuards`]).
#[instrument(fields(port, agent_config = %agent_config.display(), mode = ?mode))]
pub async fn spawn_agent(port: u16, agent_config: PathBuf, mode: GameMode) -> Result<Child> {
    let exe = std::env::current_exe().context("Failed to get current executable path")?;
    let server_url = format!("http://localhost:{}", port);

    info!("Spawning agent subprocess");

    let mut cmd = Command::new(&exe);
    cmd.arg("agent")
        .arg("--config")
        .arg(agent_config)
        .arg("--server-url")
        .arg(&server_url);

    match mode {
        GameMode::TicTacToe => {
            cmd.arg("--test-play")
                .arg("--test-session")
                .arg("tui_session");
        }
        GameMode::Blackjack {
            bankroll,
            session_id,
        } => {
            cmd.arg("--test-blackjack")
                .arg("--bankroll")
                .arg(bankroll.to_string())
                .arg("--test-session")
                .arg(session_id);
        }
    }

    let agent = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .context("Failed to spawn agent process")?;

    debug!("Agent process spawned, waiting for connection");
    sleep(Duration::from_millis(500)).await;

    info!("Agent spawned successfully");
    Ok(agent)
}

/// Polls server health endpoint until ready or timeout.
#[instrument(skip_all, fields(server_url = %server_url))]
async fn wait_for_server_ready(server_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let health_url = format!("{}/health", server_url);

    debug!(health_url = %health_url, "Starting health check polling");

    let result = timeout(Duration::from_secs(10), async {
        for attempt in 1..=20 {
            debug!(attempt, "Attempting health check");
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!("Server health check passed");
                    return Ok(());
                }
                Ok(resp) => {
                    debug!(
                        attempt,
                        status = %resp.status(),
                        "Server not ready yet"
                    );
                }
                Err(e) => {
                    debug!(
                        attempt,
                        error = %e,
                        "Server health check failed, retrying"
                    );
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
        anyhow::bail!("Server did not become ready after 20 attempts")
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Timeout waiting for server to become ready"),
    }
}
