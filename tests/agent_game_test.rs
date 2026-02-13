//! Integration test for agent playing a game.

use strictly_games::agent_config::AgentConfig;
use strictly_games::agent_handler::GameAgent;
use tokio::process::Command;
use std::process::Stdio;
use tracing_subscriber::EnvFilter;

#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
async fn test_agent_plays_game() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // Load .env
    dotenvy::dotenv().ok();

    // Create config with Anthropic
    let config = AgentConfig::with_llm(
        "TestAgent".to_string(),
        vec![
            "cargo".to_string(),
            "run".to_string(),
            "--quiet".to_string(),
            "--bin".to_string(),
            "strictly_games".to_string(),
        ],
        None,
        strictly_games::llm_client::LlmProvider::Anthropic,
        "claude-3-5-haiku-20241022".to_string(),
        150,
    );

    // Start server
    let mut server = Command::new(&config.server_command()[0])
        .args(&config.server_command()[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to start server");

    let server_stdin = server.stdin.take().expect("Failed to get stdin");
    let server_stdout = server.stdout.take().expect("Failed to get stdout");

    // Create agent
    let agent = GameAgent::new(config);
    agent.initialize_llm().await.expect("Failed to initialize LLM");

    // Connect to server
    let running_service = rmcp::serve_client(agent, (server_stdout, server_stdin))
        .await
        .expect("Failed to connect to server");

    let peer = running_service.peer();

    // List tools
    let tools = peer
        .list_tools(Default::default())
        .await
        .expect("Failed to list tools");

    eprintln!("Available tools: {}", tools.tools.len());
    for tool in &tools.tools {
        eprintln!("  - {}", tool.name);
    }

    // Call play_game tool
    let args_map: serde_json::Map<String, serde_json::Value> = serde_json::from_value(
        serde_json::json!({
            "session_id": "test_session_1",
            "player_name": "TestAgent"
        })
    ).expect("Failed to create args map");

    let result = peer
        .call_tool(rmcp::model::CallToolRequestParams {
            name: "play_game".to_string().into(),
            arguments: Some(args_map),
            task: None,
            meta: None,
        })
        .await
        .expect("Failed to call play_game");

    eprintln!("Game result: {:?}", result);

    // The game should complete (either win or draw)
    assert!(!result.content.is_empty(), "Should have game result");
}
