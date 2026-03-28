//! Strictly Games - Unified CLI
//!
//! Type-safe game server with multiple modes of operation.

#![warn(missing_docs)]
#![recursion_limit = "256"]

mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use strictly_server::{AgentConfig, AnyGame, Board, GameAgent, GameServer, SessionManager};
use tracing::{error, info, instrument, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Command::Server => {
            init_logging();
            run_mcp_server().await
        }
        Command::Http { port, host } => {
            // HTTP server sets up its own file logging
            run_http_server(host, port).await
        }
        Command::Tui {
            db_path,
            agents_dir,
            port,
        } => {
            // TUI (lobby) has its own logging setup
            let agent_config = std::path::PathBuf::from("agent_config.toml");
            run_lobby(db_path, agents_dir, port, agent_config).await
        }
        Command::Agent {
            config,
            server_url,
            server_command,
            test_play,
            test_session,
        } => {
            // Agent sets up its own file logging
            run_agent(config, server_url, server_command, test_play, test_session).await
        }
        Command::Verify { tool, verbose } => {
            init_logging();
            run_verify(&tool, verbose)
        }
    }
}

fn init_logging() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

fn run_verify(tool: &str, verbose: bool) -> Result<()> {
    match tool {
        "kani" => strictly_server::run_kani(verbose),
        "verus" => strictly_server::run_verus(verbose),
        "creusot" => strictly_server::run_creusot(verbose),
        "all" => strictly_server::run_verification_all(verbose),
        _ => anyhow::bail!("Unknown verification tool: {}", tool),
    }
}

async fn run_mcp_server() -> Result<()> {
    tracing::info!("Starting MCP server");
    let server = GameServer::new();
    rmcp::service::serve_server(server, rmcp::transport::stdio()).await?;
    Ok(())
}

/// Run the HTTP game server
#[instrument(skip_all, fields(host = %host, port))]
async fn run_http_server(host: String, port: u16) -> Result<()> {
    use axum::{Router, body::Body, http::Request};
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    };
    use std::fs::OpenOptions;
    use std::sync::Arc;
    use tower::ServiceBuilder;
    use tracing::{debug, warn};

    // Log server to file since TUI owns stdout
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("server.log")
        .expect("Failed to open server.log");

    tracing_subscriber::fmt()
        .with_writer(std::sync::Arc::new(log_file))
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,rmcp=debug")),
        )
        .with_ansi(false)
        .try_init()
        .ok(); // Ignore error if already initialized

    info!("Starting Strictly Games MCP server on HTTP");
    info!(port, "Server will listen on http://localhost:{}", port);

    let session_manager = Arc::new(LocalSessionManager::default());

    // Create SHARED SessionManager for game state (already has Arc<Mutex<>> internally)
    let game_sessions = SessionManager::new();

    // Configure for STATEFUL mode (required for elicitation loops)
    let config = StreamableHttpServerConfig {
        stateful_mode: true, // Keep connections alive for bidirectional communication
        ..Default::default()
    };
    debug!(?config, "HTTP service configuration");

    // Clone sessions for different uses (cheap - clones internal Arc)
    let rest_sessions = game_sessions.clone();
    let mcp_game_sessions = game_sessions.clone();

    debug!("About to create StreamableHttpService");

    // Factory creates GameServer that shares session state
    let http_service = StreamableHttpService::new(
        move || {
            debug!("Creating new GameServer instance with shared sessions");
            Ok(GameServer::with_sessions(mcp_game_sessions.clone()))
        },
        session_manager.clone(),
        config,
    );

    debug!("StreamableHttpService created successfully");

    // Build app with REST API and MCP fallback
    let app = Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .route(
            "/api/sessions/{session_id}/game",
            axum::routing::get({
                let sessions = rest_sessions.clone();
                move |axum::extract::Path(session_id): axum::extract::Path<String>| async move {
                    use axum::Json;
                    if let Some(session) = sessions.get_session(&session_id) {
                        Json(session.game.clone())
                    } else {
                        Json(AnyGame::Setup {
                            board: Board::default(),
                        })
                    }
                }
            }),
        )
        .route(
            "/api/sessions/{session_id}/restart",
            axum::routing::post({
                move |axum::extract::Path(session_id): axum::extract::Path<String>| async move {
                    use axum::http::StatusCode;
                    match rest_sessions.restart_game(&session_id) {
                        Ok(()) => StatusCode::OK,
                        Err(_) => StatusCode::NOT_FOUND,
                    }
                }
            }),
        )
        .fallback_service(
            ServiceBuilder::new()
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
                })),
        );

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

/// Run the lobby TUI
#[instrument(skip_all, fields(db_path = %db_path, port))]
async fn run_lobby(
    db_path: String,
    agents_dir: Option<std::path::PathBuf>,
    port: u16,
    agent_config: std::path::PathBuf,
) -> Result<()> {
    use ratatui::{Terminal, backend::CrosstermBackend};
    use std::io;
    use strictly_server::{AgentLibrary, GameRepository, LobbyController, ProfileService};

    // Setup logging to file
    let log_file = std::fs::File::create("strictly_games_lobby.log")?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_writer(std::sync::Arc::new(log_file))
        .with_ansi(false)
        .try_init();

    info!("Starting lobby");

    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup services
    info!(db_path = %db_path, "Initializing game repository");
    let repository = GameRepository::new(db_path)?;
    let profile_service = ProfileService::new(repository.clone());

    info!(agents_dir = ?agents_dir, "Loading agent library");
    let agent_library = if let Some(dir) = agents_dir {
        info!(path = %dir.display(), "Scanning custom agent directory");
        AgentLibrary::scan(&dir).unwrap_or_else(|e| {
            warn!(error = %e, "Failed to scan agent directory, trying default");
            AgentLibrary::scan_default().unwrap_or_else(|e2| {
                warn!(error = %e2, "Failed to load default agents, using empty library");
                AgentLibrary::empty()
            })
        })
    } else {
        info!("No agents_dir specified, scanning default location");
        match AgentLibrary::scan_default() {
            Ok(lib) => {
                info!(count = lib.count(), "Loaded agents from default location");
                lib
            }
            Err(e) => {
                warn!(error = %e, "Failed to scan default agents, using empty library");
                AgentLibrary::empty()
            }
        }
    };

    info!(
        agent_count = agent_library.count(),
        "Agent library initialized"
    );

    // Run lobby
    let mut controller = LobbyController::new(profile_service, agent_library, agent_config, port);

    let result = controller.run(&mut terminal).await;

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
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
    // Load .env file (needed when run as subprocess)
    dotenvy::dotenv().ok();

    initialize_agent_tracing();

    info!("Starting MCP agent");

    // Load configuration
    let config = load_agent_config(&config, server_command)?;
    info!(config_name = %config.name(), "Config loaded");

    // Create handler
    let handler = GameAgent::new(config.clone());
    info!("Handler created");

    // Initialize LLM client
    info!("Initializing LLM client");
    handler.initialize_llm().await.map_err(|e| {
        error!(error = %e, "LLM init failed");
        anyhow::anyhow!(e)
    })?;
    info!("LLM initialized");

    // Connect to server (either HTTP or stdio)
    let running_service = if let Some(server_url) = &server_url {
        // HTTP mode
        info!(url = %server_url, "Connecting to HTTP MCP server");
        let svc = connect_http(handler, server_url).await?;
        info!("Connected to HTTP server");
        svc
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
        let session_id =
            test_session.unwrap_or_else(|| format!("auto_game_{}", std::process::id()));

        // Continuously play games until Ctrl+C
        loop {
            info!("Starting new game session");
            match test_play_game(peer, &config, &session_id).await {
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
    config: &AgentConfig,
    session_id: &str,
) -> Result<()> {
    use serde_json::json;

    info!(session_id, player_name = %config.name(), "test_play_game: Calling play_game tool");

    let result = peer
        .call_tool(
            rmcp::model::CallToolRequestParams::new("play_game").with_arguments(
                json!({
                    "session_id": session_id,
                    "player_name": config.name()
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await?;

    info!(result = ?result, "test_play_game: play_game completed");
    Ok(())
}

#[instrument(skip(handler))]
async fn connect_http(
    handler: GameAgent,
    url: &str,
) -> Result<rmcp::service::RunningService<rmcp::RoleClient, GameAgent>> {
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
) -> Result<AgentConfig> {
    info!("Loading agent configuration");

    let mut config = if config_path.exists() {
        AgentConfig::from_file(config_path)?
    } else {
        info!(
            "Config file not found at {}, using defaults",
            config_path.display()
        );
        AgentConfig::new(
            "Haiku (Fast)".to_string(),
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
        config = AgentConfig::new(config.name().clone(), parts, config.server_cwd().clone());
    }

    Ok(config)
}

#[instrument(skip(config))]
async fn start_server(
    config: &AgentConfig,
) -> Result<(tokio::process::ChildStdin, tokio::process::ChildStdout)> {
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
    use std::fs::OpenOptions;
    use tracing_subscriber::fmt::format::FmtSpan;

    // Log agent to file since TUI owns stderr
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("agent.log")
        .expect("Failed to open agent.log");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,strictly_games=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Arc::new(log_file))
                .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
                .with_line_number(true)
                .with_thread_ids(true)
                .with_ansi(false),
        )
        .try_init()
        .ok(); // Ignore error if already initialized

    info!("Agent tracing initialized, logging to agent.log");
}
