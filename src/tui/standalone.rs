//! Standalone mode subprocess management.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration, timeout};
use tracing::{debug, info, warn, instrument};

/// Guards for spawned subprocesses. Kills processes on drop.
pub struct ProcessGuards {
    server: Option<Child>,
    agent: Option<Child>,
}

impl ProcessGuards {
    fn new(server: Child, agent: Child) -> Self {
        Self {
            server: Some(server),
            agent: Some(agent),
        }
    }
}

impl Drop for ProcessGuards {
    fn drop(&mut self) {
        info!("Cleaning up spawned subprocesses");
        
        if let Some(mut agent) = self.agent.take() {
            debug!("Killing agent process");
            let _ = agent.start_kill();
        }
        
        if let Some(mut server) = self.server.take() {
            debug!("Killing server process");
            let _ = server.start_kill();
        }
    }
}

/// Spawns server and agent subprocesses for standalone mode.
#[instrument(skip_all, fields(port, agent_config = %agent_config.display()))]
pub async fn spawn_standalone(port: u16, agent_config: PathBuf) -> Result<ProcessGuards> {
    info!("Starting standalone mode: spawning server and agent");
    
    // Get the path to the current executable
    let exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    // Spawn HTTP server
    info!(port, "Spawning HTTP game server");
    let server = Command::new(&exe)
        .arg("http")
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn server process")?;
    
    debug!("Server process spawned, waiting for readiness");
    
    // Wait for server to be ready
    let server_url = format!("http://localhost:{}", port);
    wait_for_server_ready(&server_url).await
        .context("Server failed to become ready")?;
    
    info!("Server is ready, spawning agent");
    
    // Spawn agent connected to the server, joining the TUI session
    let agent = Command::new(&exe)
        .arg("agent")
        .arg("--config")
        .arg(agent_config)
        .arg("--server-url")
        .arg(&server_url)
        .arg("--test-play")
        .arg("--test-session")
        .arg("tui_session")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())  // Let agent logs flow to same output
        .spawn()
        .context("Failed to spawn agent process")?;
    
    debug!("Agent process spawned");
    
    // Give agent a moment to connect
    sleep(Duration::from_millis(500)).await;
    
    info!("Standalone mode initialized successfully");
    
    Ok(ProcessGuards::new(server, agent))
}

/// Polls server health endpoint until ready or timeout.
#[instrument(skip_all, fields(server_url = %server_url))]
async fn wait_for_server_ready(server_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let health_url = format!("{}/health", server_url);
    
    let result = timeout(Duration::from_secs(10), async {
        for attempt in 1..=20 {
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
    }).await;
    
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Timeout waiting for server to become ready"),
    }
}
