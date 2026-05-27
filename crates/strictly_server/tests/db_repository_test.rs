//! Integration tests for [`GameRepository`] using in-memory redb.

use strictly_server::{GameRepository, NewGameStat};

fn make_stat(user_id: &str, outcome: &str) -> NewGameStat {
    NewGameStat::new(
        user_id.to_string(),
        "opponent".to_string(),
        "tictactoe".to_string(),
        outcome.to_string(),
        5,
        "test_session".to_string(),
    )
}

#[tokio::test]
async fn create_and_retrieve_user() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    let user = repo.create_user("Alice".to_string()).await.expect("create");
    assert_eq!(user.display_name(), "Alice");

    let found = repo
        .get_user_by_name("Alice")
        .await
        .expect("lookup")
        .expect("present");
    assert_eq!(found.id(), user.id());
}

#[tokio::test]
async fn duplicate_name_rejected() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    repo.create_user("Bob".to_string()).await.expect("first");
    let result = repo.create_user("Bob".to_string()).await;
    assert!(result.is_err(), "duplicate should fail");
}

#[tokio::test]
async fn list_users_sorted_by_creation() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    repo.create_user("Alpha".to_string()).await.expect("1");
    repo.create_user("Beta".to_string()).await.expect("2");
    repo.create_user("Gamma".to_string()).await.expect("3");

    let users = repo.list_users().await.expect("list");
    assert_eq!(users.len(), 3);
    assert_eq!(users[0].display_name(), "Alpha");
    assert_eq!(users[1].display_name(), "Beta");
    assert_eq!(users[2].display_name(), "Gamma");
}

#[tokio::test]
async fn record_game_and_aggregate() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    let user = repo.create_user("Carol".to_string()).await.expect("create");
    let uid = user.id().as_str();

    repo.record_game(make_stat(uid, "win")).await.expect("win");
    repo.record_game(make_stat(uid, "win")).await.expect("win");
    repo.record_game(make_stat(uid, "loss")).await.expect("loss");
    repo.record_game(make_stat(uid, "draw")).await.expect("draw");

    let stats = repo.get_aggregated_stats(uid).await.expect("stats");
    assert_eq!(*stats.total_games(), 4);
    assert_eq!(*stats.wins(), 2);
    assert_eq!(*stats.losses(), 1);
    assert_eq!(*stats.draws(), 1);
    assert!((stats.win_rate() - 50.0).abs() < 0.01);
}

#[tokio::test]
async fn get_user_stats_most_recent_first() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    let user = repo.create_user("Dave".to_string()).await.expect("create");
    let uid = user.id().as_str();

    repo.record_game(make_stat(uid, "win")).await.expect("first");
    repo.record_game(make_stat(uid, "loss")).await.expect("second");

    let history = repo.get_user_stats(uid).await.expect("history");
    assert_eq!(history.len(), 2);
    // Most recent recorded last should sort to front.
    assert!(history[0].played_at() >= history[1].played_at());
}

#[tokio::test]
async fn get_user_by_id_missing_returns_none() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    let result = repo
        .get_user_by_id("00000000-0000-0000-0000-000000000000")
        .await
        .expect("no error");
    assert!(result.is_none());
}

#[tokio::test]
async fn aggregated_stats_empty_user() {
    let repo = GameRepository::in_memory().expect("in-memory repo");
    let user = repo.create_user("Eve".to_string()).await.expect("create");
    let stats = repo
        .get_aggregated_stats(user.id().as_str())
        .await
        .expect("stats");
    assert_eq!(*stats.total_games(), 0);
    assert!((stats.win_rate() - 0.0).abs() < 0.01);
}
