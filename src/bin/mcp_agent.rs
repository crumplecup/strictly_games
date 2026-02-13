//! MCP agent client binary.

use clap::Parser;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use strictly_games::agent_config::AgentConfig;
use strictly_games::agent_handler::GameAgent;

/// MCP agent client for playing games.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to agent configuration file
    #[arg(short, long, default_value = "agent_config.toml")]
    config: PathBuf,

    /// Server URL (HTTP mode) - if not provided, spawns server via stdio
    #[arg(long)]
    server_url: Option<String>,

    /// Override server command (space-separated, stdio mode only)
    #[arg(short, long)]
    server_command: Option<String>,

    /// Auto-trigger play_game tool for testing
    #[arg(long)]
    test_play: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file
    dotenvy::dotenv().ok();

    initialize_tracing();

    let result = run().await;

    if let Err(e) = &result {
        tracing::error!(error = ?e, "Agent failed");
    }

    result
}

#[instrument]
async fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    info!(config_path = %args.config.display(), "Starting MCP agent");

    // Load configuration
    let config = load_config(&args)?;

    // Create handler
    let handler = GameAgent::new(config.clone());

    // Initialize LLM client
    info!("Initializing LLM client");
    handler.initialize_llm().await.map_err(|e| anyhow::anyhow!(e))?;

    // Connect to server (either HTTP or stdio)
    let running_service = if let Some(server_url) = &args.server_url {
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
    if args.test_play {
        info!("Test mode: calling play_game tool");
        test_play_game(&peer, &config).await?;
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
) -> anyhow::Result<()> {
    use serde_json::json;

    info!("Calling play_game tool for testing");

    let result = peer
        .call_tool(rmcp::model::CallToolRequestParams {
            name: "play_game".into(),
            arguments: Some(json!({
                "session_id": "test_game",
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
    handler: GameAgent,
    url: &str,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, GameAgent>> {
    use rmcp::transport::StreamableHttpClientTransport;
    
    info!(url, "Creating HTTP transport");
    let transport = StreamableHttpClientTransport::from_uri(url);
    
    info!("Connecting to HTTP server");
    let running_service = rmcp::serve_client(handler, transport).await?;
    
    Ok(running_service)
}

#[instrument(skip(args))]
fn load_config(args: &Args) -> anyhow::Result<AgentConfig> {
    info!("Loading configuration");

    let mut config = if args.config.exists() {
        AgentConfig::from_file(&args.config)?
    } else {
        info!(
            "Config file not found at {}, using defaults",
            args.config.display()
        );
        AgentConfig::new(
            "Agent_1".to_string(),
            vec![
                "cargo".to_string(),
                "run".to_string(),
                "--bin".to_string(),
                "strictly_games".to_string(),
            ],
            None,
        )
    };

    // Override server command if provided
    if let Some(cmd) = &args.server_command {
        info!(command = %cmd, "Overriding server command");
        let parts: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
        config = AgentConfig::new(config.name().clone(), parts, config.server_cwd().clone());
    }

    Ok(config)
}

#[instrument(skip(config))]
async fn start_server(
    config: &AgentConfig,
) -> anyhow::Result<(
    tokio::process::ChildStdin,
    tokio::process::ChildStdout,
)> {
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
fn initialize_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,strictly_games=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Tracing initialized");
}
