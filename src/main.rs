//! Strictly Games - Unified CLI
//!
//! Type-safe game server with multiple modes of operation.

#![warn(missing_docs)]

mod agent_config;
mod agent_handler;
mod cli;
mod games;
mod llm_client;
mod server;
mod session;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use rmcp::ServiceExt;
use server::GameServer;
use tracing::{info, instrument};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Command::Server => run_mcp_server().await,
        Command::Http { port, host } => run_http_server(host, port).await,
        Command::Tui { server_url } => run_tui(server_url).await,
        Command::Agent {
            config,
            server_url,
            server_command,
            test_play,
            test_session,
        } => run_agent(config, server_url, server_command, test_play, test_session).await,
    }
}

/// Run the MCP game server (stdio mode)
async fn run_mcp_server() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting Strictly Games MCP server");

    let server = GameServer::new();
    
    info!("Server ready - connect via MCP protocol");
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;

    Ok(())
}

/// Run the HTTP game server
async fn run_http_server(host: String, port: u16) -> Result<()> {
    use axum::{body::Body, http::Request, Router};
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    };
    use std::sync::Arc;
    use tower::ServiceBuilder;
    use tracing::{debug, warn};

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rmcp=debug"))
        )
        .init();

    info!("Starting Strictly Games MCP server on HTTP");
    info!(port, "Server will listen on http://localhost:{}", port);

    let session_manager = Arc::new(LocalSessionManager::default());
    
    // Create SHARED SessionManager for game state (Arc for multi-request sharing)
    let game_sessions = Arc::new(session::SessionManager::new());
    
    // Configure for STATEFUL mode (required for elicitation loops)
    let mut config = StreamableHttpServerConfig::default();
    config.stateful_mode = true;  // Keep connections alive for bidirectional communication
    debug!(?config, "HTTP service configuration");
    
    // Factory creates GameServer that shares session state
    let http_service = StreamableHttpService::new(
        move || {
            debug!("Creating new GameServer instance with shared sessions");
            Ok(GameServer::with_sessions((*game_sessions).clone()))
        },
        session_manager,
        config,
    );
    
    // Wrap service with request logging
    let app = Router::new()
        .fallback_service(ServiceBuilder::new()
            .map_request(|req: Request<Body>| {
                info!(
                    method = %req.method(),
                    uri = %req.uri(),
                    headers = ?req.headers(),
                    "Incoming HTTP request"
                );
                req
            })
            .service(tower::service_fn(move |req: Request<Body>| {
                let mut service = http_service.clone();
                async move {
                    let uri = req.uri().clone();
                    let result = tower::Service::call(&mut service, req).await;
                    match &result {
                        Ok(resp) => info!(status = ?resp.status(), uri = %uri, "Response sent"),
                        Err(e) => warn!(error = ?e, uri = %uri, "Request failed"),
                    }
                    result
                }
            })));
    
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    info!("✅ Server ready at http://localhost:{}/", port);
    info!("📡 Accepting SSE connections");
    info!("🎮 Tools: start_game, get_board, make_move");
    info!("🔍 Trace logging enabled - all requests will be logged");
    
    info!("🔄 About to call axum::serve() - this should block forever");
    let result = axum::serve(listener, app).await;
    info!("❌ axum::serve() returned! This should never happen!");
    info!(?result, "Server exited with result");
    result?;

    Ok(())
}

/// Run the TUI client
async fn run_tui(server_url: String) -> Result<()> {
    tui::run_tui(server_url).await
}

/// Run the MCP agent
#[instrument(skip_all, fields(config_path = %config.display()))]
async fn run_agent(
    config: std::path::PathBuf,
    server_url: Option<String>,
    server_command: Option<String>,
    test_play: bool,
    test_session: Option<String>,
) -> Result<()> {
    initialize_agent_tracing();
    
    info!("Starting MCP agent");
    
    // Load configuration
    let config = load_agent_config(&config, server_command)?;
    
    // Create handler
    let handler = agent_handler::GameAgent::new(config.clone());
    
    // Initialize LLM client
    info!("Initializing LLM client");
    handler.initialize_llm().await.map_err(|e| anyhow::anyhow!(e))?;
    
    // Connect to server (either HTTP or stdio)
    let running_service = if let Some(server_url) = &server_url {
        // HTTP mode
        info!(url = %server_url, "Connecting to HTTP MCP server");
        connect_http(handler, server_url).await?
    } else {
        // Stdio mode (spawn server)
        info!("Starting server process for stdio connection");
        let (server_stdin, server_stdout) = start_server(&config).await?;
        info!("Connecting to MCP server via stdio");
        rmcp::serve_client(handler, (server_stdout, server_stdin)).await?
    };
    
    info!("Agent connected successfully, peer created");
    let peer = running_service.peer();
    
    // List available tools
    info!("Listing available tools");
    let tools = peer.list_tools(Default::default()).await?;
    info!(tool_count = tools.tools.len(), "Tools discovered");
    for tool in &tools.tools {
        info!(tool_name = %tool.name, "Available tool");
    }
    
    // If --test-play flag is set, call play_game tool
    if test_play {
        info!("Test mode: calling play_game tool in continuous loop");
        let session_id = test_session
            .unwrap_or_else(|| format!("auto_game_{}", std::process::id()));
        
        // Continuously play games until Ctrl+C
        loop {
            info!("Starting new game session");
            match test_play_game(&peer, &config, &session_id).await {
                Ok(_) => {
                    info!("Game completed, waiting for next game to start");
                    // Small delay before checking for next game
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "play_game failed, retrying");
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    } else {
        // Keep running normally
        info!("Agent running. Press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await?;
        info!("Shutting down agent");
    }
    
    Ok(())
}

#[instrument(skip(peer, config))]
async fn test_play_game(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    config: &agent_config::AgentConfig,
    session_id: &str,
) -> Result<()> {
    use serde_json::json;
    
    info!(session_id, "Calling play_game tool");
    
    let result = peer
        .call_tool(rmcp::model::CallToolRequestParams {
            name: "play_game".into(),
            arguments: Some(json!({
                "session_id": session_id,
                "player_name": config.name()
            }).as_object().unwrap().clone()),
            task: None,
            meta: None,
        })
        .await?;
    
    info!(result = ?result, "play_game completed");
    Ok(())
}

#[instrument(skip(handler))]
async fn connect_http(
    handler: agent_handler::GameAgent,
    url: &str,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, agent_handler::GameAgent>> {
    use rmcp::transport::StreamableHttpClientTransport;
    
    info!(url, "Creating HTTP transport");
    let transport = StreamableHttpClientTransport::from_uri(url);
    
    info!("Connecting to HTTP server");
    let running_service = rmcp::serve_client(handler, transport).await?;
    
    Ok(running_service)
}

#[instrument(skip(config_path))]
fn load_agent_config(
    config_path: &std::path::Path,
    server_command_override: Option<String>,
) -> Result<agent_config::AgentConfig> {
    info!("Loading agent configuration");
    
    let mut config = if config_path.exists() {
        agent_config::AgentConfig::from_file(config_path)?
    } else {
        info!(
            "Config file not found at {}, using defaults",
            config_path.display()
        );
        agent_config::AgentConfig::new(
            "Agent_1".to_string(),
            vec![
                "cargo".to_string(),
                "run".to_string(),
                "--".to_string(),
                "server".to_string(),
            ],
            None,
        )
    };
    
    // Override server command if provided
    if let Some(cmd) = server_command_override {
        info!(command = %cmd, "Overriding server command");
        let parts: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        config = agent_config::AgentConfig::new(config.name().clone(), parts, config.server_cwd().clone());
    }
    
    Ok(config)
}

#[instrument(skip(config))]
async fn start_server(
    config: &agent_config::AgentConfig,
) -> Result<(
    tokio::process::ChildStdin,
    tokio::process::ChildStdout,
)> {
    use std::process::Stdio;
    use tokio::process::Command;
    
    let cmd = &config.server_command()[0];
    let args = &config.server_command()[1..];
    
    info!(command = %cmd, args = ?args, "Starting MCP server process");
    
    let mut command = Command::new(cmd);
    command.args(args);
    
    if let Some(cwd) = config.server_cwd() {
        command.current_dir(cwd);
    }
    
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    
    let stdin = child.stdin.take().ok_or_else(|| {
        tracing::error!("Failed to capture server stdin");
        anyhow::anyhow!("Failed to capture stdin")
    })?;
    
    let stdout = child.stdout.take().ok_or_else(|| {
        tracing::error!("Failed to capture server stdout");
        anyhow::anyhow!("Failed to capture stdout")
    })?;
    
    info!("Server process started");
    
    Ok((stdin, stdout))
}

#[instrument]
fn initialize_agent_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,strictly_games=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Agent tracing initialized");
}
