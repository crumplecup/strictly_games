//! Integration test for LLM client connectivity.

use strictly_games::llm_client::{LlmClient, LlmConfig, LlmProvider};
use tracing::instrument;

#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
#[instrument]
async fn test_anthropic_connectivity() {
    dotenvy::dotenv().ok();
    
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

    let config = LlmConfig::new(
        LlmProvider::Anthropic,
        api_key,
        "claude-3-5-haiku-20241022".to_string(),
        50,
    );

    let client = LlmClient::new(config);

    let response = client
        .generate("You are a helpful assistant.", "Say 'Hello, world!' and nothing else.")
        .await
        .expect("Failed to generate");

    assert!(!response.is_empty(), "Response should not be empty");
    eprintln!("Response: {}", response);
}

#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
#[instrument]
async fn test_openai_connectivity() {
    dotenvy::dotenv().ok();
    
    let api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");

    let config = LlmConfig::new(
        LlmProvider::OpenAI,
        api_key,
        "gpt-4o-mini".to_string(),
        50,
    );

    let client = LlmClient::new(config);

    let response = client
        .generate("You are a helpful assistant.", "Say 'Hello, world!' and nothing else.")
        .await
        .expect("Failed to generate");

    assert!(!response.is_empty(), "Response should not be empty");
    eprintln!("Response: {}", response);
}
