//! Test all current Claude 4.x models to verify API compatibility.

use strictly_games::{LlmClient, LlmConfig, LlmProvider};

/// Test that Claude Haiku 4.5 model is accessible.
#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
async fn test_claude_haiku_4_5() {
    dotenvy::dotenv().ok();
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
    let config = LlmConfig::new(
        LlmProvider::Anthropic,
        api_key,
        "claude-haiku-4-5-20251001".to_string(),
        10, // Minimal tokens for quick test
    );

    let client = LlmClient::new(config);
    let response = client
        .generate("You are a test assistant.", "Respond with exactly: OK")
        .await
        .expect("Haiku 4.5 should respond");

    assert!(!response.is_empty(), "Response should not be empty");
}

/// Test that Claude Sonnet 4.6 model is accessible.
#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
async fn test_claude_sonnet_4_6() {
    dotenvy::dotenv().ok();
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
    let config = LlmConfig::new(
        LlmProvider::Anthropic,
        api_key,
        "claude-sonnet-4-6".to_string(),
        10,
    );

    let client = LlmClient::new(config);
    let response = client
        .generate("You are a test assistant.", "Respond with exactly: OK")
        .await
        .expect("Sonnet 4.6 should respond");

    assert!(!response.is_empty(), "Response should not be empty");
}

/// Test that Claude Opus 4.6 model is accessible.
#[tokio::test]
#[cfg_attr(not(feature = "api"), ignore)]
async fn test_claude_opus_4_6() {
    dotenvy::dotenv().ok();
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");
    let config = LlmConfig::new(
        LlmProvider::Anthropic,
        api_key,
        "claude-opus-4-6".to_string(),
        10,
    );

    let client = LlmClient::new(config);
    let response = client
        .generate("You are a test assistant.", "Respond with exactly: OK")
        .await
        .expect("Opus 4.6 should respond");

    assert!(!response.is_empty(), "Response should not be empty");
}
