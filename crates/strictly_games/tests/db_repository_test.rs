//! Tests for database repository operations.

use strictly_games::{GameOutcome, GameRepository, NewGameStat};

/// Creates an in-memory repository for isolated testing.
fn setup_test_db() -> GameRepository {
    GameRepository::in_memory().expect("in-memory repository")
}

#[tokio::test]
async fn test_create_user() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Alice".to_string())
        .await
        .expect("Create failed");
    assert_eq!(user.display_name(), "Alice");
    assert!(!user.id().is_empty());
}

#[tokio::test]
async fn test_create_user_duplicate_name_fails() {
    let repo = setup_test_db();
    repo.create_user("Bob".to_string())
        .await
        .expect("First create failed");
    let result = repo.create_user("Bob".to_string()).await;
    assert!(result.is_err(), "Duplicate name should fail");
}

#[tokio::test]
async fn test_get_user_by_name_found() {
    let repo = setup_test_db();
    repo.create_user("Carol".to_string())
        .await
        .expect("Create failed");
    let found = repo.get_user_by_name("Carol").await.expect("Query failed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().display_name(), "Carol");
}

#[tokio::test]
async fn test_get_user_by_name_not_found() {
    let repo = setup_test_db();
    let found = repo
        .get_user_by_name("NoSuchUser")
        .await
        .expect("Query failed");
    assert!(found.is_none());
}

#[tokio::test]
async fn test_list_users_empty() {
    let repo = setup_test_db();
    let users = repo.list_users().await.expect("List failed");
    assert!(users.is_empty());
}

#[tokio::test]
async fn test_list_users_ordered_by_creation() {
    let repo = setup_test_db();
    repo.create_user("Alpha".to_string())
        .await
        .expect("Create failed");
    repo.create_user("Beta".to_string())
        .await
        .expect("Create failed");
    repo.create_user("Gamma".to_string())
        .await
        .expect("Create failed");

    let users = repo.list_users().await.expect("List failed");
    assert_eq!(users.len(), 3);
}

#[tokio::test]
async fn test_record_game() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Dave".to_string())
        .await
        .expect("Create failed");

    let stat = NewGameStat::new(
        user.id().clone(),
        "Agent1".to_string(),
        "tictactoe".to_string(),
        GameOutcome::Win.to_db_string().to_string(),
        7,
        "session_001".to_string(),
    );

    let recorded = repo.record_game(stat).await.expect("Record failed");
    assert_eq!(recorded.user_id(), user.id());
    assert_eq!(recorded.outcome(), "win");
    assert_eq!(*recorded.moves_count(), 7);
    assert_eq!(recorded.opponent_name(), "Agent1");
}

#[tokio::test]
async fn test_get_user_stats() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Eve".to_string())
        .await
        .expect("Create failed");

    for outcome in &["win", "loss", "draw"] {
        let stat = NewGameStat::new(
            user.id().clone(),
            "Bot".to_string(),
            "tictactoe".to_string(),
            outcome.to_string(),
            5,
            format!("session_{outcome}"),
        );
        repo.record_game(stat).await.expect("Record failed");
    }

    let stats = repo.get_user_stats(user.id()).await.expect("Stats failed");
    assert_eq!(stats.len(), 3);
}

#[tokio::test]
async fn test_get_aggregated_stats() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Frank".to_string())
        .await
        .expect("Create failed");

    let outcomes = [("win", 3), ("loss", 1), ("draw", 2)];
    for (outcome, count) in &outcomes {
        for i in 0..*count {
            let stat = NewGameStat::new(
                user.id().clone(),
                "Opponent".to_string(),
                "tictactoe".to_string(),
                outcome.to_string(),
                5,
                format!("session_{outcome}_{i}"),
            );
            repo.record_game(stat).await.expect("Record failed");
        }
    }

    let agg = repo
        .get_aggregated_stats(user.id())
        .await
        .expect("Aggregation failed");
    assert_eq!(*agg.total_games(), 6);
    assert_eq!(*agg.wins(), 3);
    assert_eq!(*agg.losses(), 1);
    assert_eq!(*agg.draws(), 2);
    assert!((agg.win_rate() - 50.0).abs() < 0.001);
}

#[tokio::test]
async fn test_get_aggregated_stats_no_games() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Grace".to_string())
        .await
        .expect("Create failed");

    let agg = repo
        .get_aggregated_stats(user.id())
        .await
        .expect("Aggregation failed");
    assert_eq!(*agg.total_games(), 0);
    assert_eq!(agg.win_rate(), 0.0);
}

#[tokio::test]
async fn test_get_stats_by_opponent() {
    let repo = setup_test_db();
    let user = repo
        .create_user("Hank".to_string())
        .await
        .expect("Create failed");

    for opponent in &["AlphaBot", "AlphaBot", "BetaBot"] {
        let stat = NewGameStat::new(
            user.id().clone(),
            opponent.to_string(),
            "tictactoe".to_string(),
            "win".to_string(),
            5,
            format!("session_{opponent}"),
        );
        repo.record_game(stat).await.expect("Record failed");
    }

    let alpha_stats = repo
        .get_stats_by_opponent(user.id(), "AlphaBot")
        .await
        .expect("Stats failed");
    assert_eq!(alpha_stats.len(), 2);

    let beta_stats = repo
        .get_stats_by_opponent(user.id(), "BetaBot")
        .await
        .expect("Stats failed");
    assert_eq!(beta_stats.len(), 1);

    let none_stats = repo
        .get_stats_by_opponent(user.id(), "GammaBot")
        .await
        .expect("Stats failed");
    assert!(none_stats.is_empty());
}

#[tokio::test]
async fn test_game_outcome_round_trip() {
    for outcome in &[GameOutcome::Win, GameOutcome::Loss, GameOutcome::Draw] {
        let s = outcome.to_db_string();
        let parsed = GameOutcome::from_db_string(s).expect("Parse failed");
        assert_eq!(*outcome, parsed);
    }
}

#[test]
fn test_game_outcome_invalid_string() {
    let result = GameOutcome::from_db_string("invalid");
    assert!(result.is_err());
}
