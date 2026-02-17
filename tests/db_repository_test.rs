//! Tests for database repository operations.

use diesel::Connection;
use diesel::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use tempfile::NamedTempFile;

use strictly_games::{GameOutcome, GameRepository, NewGameStat};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Creates a temporary database file with schema applied, returns the file
/// handle (must stay in scope to keep the file alive) and a ready repository.
fn setup_test_db() -> (NamedTempFile, GameRepository) {
    let db_file = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = db_file.path().to_str().expect("Invalid path").to_string();

    let mut conn = SqliteConnection::establish(&db_path).expect("Failed to connect");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Migrations failed");

    let repo = GameRepository::new(db_path).expect("Failed to create repository");
    (db_file, repo)
}

#[test]
fn test_create_user() {
    let (_db, repo) = setup_test_db();
    let user = repo
        .create_user("Alice".to_string())
        .expect("Create failed");
    assert_eq!(user.display_name(), "Alice");
    assert!(*user.id() > 0);
}

#[test]
fn test_create_user_duplicate_name_fails() {
    let (_db, repo) = setup_test_db();
    repo.create_user("Bob".to_string())
        .expect("First create failed");
    let result = repo.create_user("Bob".to_string());
    assert!(result.is_err(), "Duplicate name should fail");
}

#[test]
fn test_get_user_by_name_found() {
    let (_db, repo) = setup_test_db();
    repo.create_user("Carol".to_string())
        .expect("Create failed");
    let found = repo.get_user_by_name("Carol").expect("Query failed");
    assert!(found.is_some());
    assert_eq!(found.unwrap().display_name(), "Carol");
}

#[test]
fn test_get_user_by_name_not_found() {
    let (_db, repo) = setup_test_db();
    let found = repo.get_user_by_name("NoSuchUser").expect("Query failed");
    assert!(found.is_none());
}

#[test]
fn test_list_users_empty() {
    let (_db, repo) = setup_test_db();
    let users = repo.list_users().expect("List failed");
    assert!(users.is_empty());
}

#[test]
fn test_list_users_ordered_by_creation() {
    let (_db, repo) = setup_test_db();
    repo.create_user("Alpha".to_string())
        .expect("Create failed");
    repo.create_user("Beta".to_string()).expect("Create failed");
    repo.create_user("Gamma".to_string())
        .expect("Create failed");

    let users = repo.list_users().expect("List failed");
    assert_eq!(users.len(), 3);
    assert_eq!(users[0].display_name(), "Alpha");
    assert_eq!(users[1].display_name(), "Beta");
    assert_eq!(users[2].display_name(), "Gamma");
}

#[test]
fn test_record_game() {
    let (_db, repo) = setup_test_db();
    let user = repo.create_user("Dave".to_string()).expect("Create failed");

    let stat = NewGameStat::new(
        *user.id(),
        "Agent1".to_string(),
        "tictactoe".to_string(),
        GameOutcome::Win.to_db_string().to_string(),
        7,
        "session_001".to_string(),
    );

    let recorded = repo.record_game(stat).expect("Record failed");
    assert_eq!(recorded.user_id(), user.id());
    assert_eq!(recorded.outcome(), "win");
    assert_eq!(*recorded.moves_count(), 7);
    assert_eq!(recorded.opponent_name(), "Agent1");
}

#[test]
fn test_get_user_stats() {
    let (_db, repo) = setup_test_db();
    let user = repo.create_user("Eve".to_string()).expect("Create failed");

    for outcome in &["win", "loss", "draw"] {
        let stat = NewGameStat::new(
            *user.id(),
            "Bot".to_string(),
            "tictactoe".to_string(),
            outcome.to_string(),
            5,
            format!("session_{outcome}"),
        );
        repo.record_game(stat).expect("Record failed");
    }

    let stats = repo.get_user_stats(*user.id()).expect("Stats failed");
    assert_eq!(stats.len(), 3);
}

#[test]
fn test_get_aggregated_stats() {
    let (_db, repo) = setup_test_db();
    let user = repo
        .create_user("Frank".to_string())
        .expect("Create failed");

    let outcomes = [("win", 3), ("loss", 1), ("draw", 2)];
    for (outcome, count) in &outcomes {
        for i in 0..*count {
            let stat = NewGameStat::new(
                *user.id(),
                "Opponent".to_string(),
                "tictactoe".to_string(),
                outcome.to_string(),
                5,
                format!("session_{outcome}_{i}"),
            );
            repo.record_game(stat).expect("Record failed");
        }
    }

    let agg = repo
        .get_aggregated_stats(*user.id())
        .expect("Aggregation failed");
    assert_eq!(*agg.total_games(), 6);
    assert_eq!(*agg.wins(), 3);
    assert_eq!(*agg.losses(), 1);
    assert_eq!(*agg.draws(), 2);
    assert!((agg.win_rate() - 50.0).abs() < 0.001);
}

#[test]
fn test_get_aggregated_stats_no_games() {
    let (_db, repo) = setup_test_db();
    let user = repo
        .create_user("Grace".to_string())
        .expect("Create failed");

    let agg = repo
        .get_aggregated_stats(*user.id())
        .expect("Aggregation failed");
    assert_eq!(*agg.total_games(), 0);
    assert_eq!(agg.win_rate(), 0.0);
}

#[test]
fn test_get_stats_by_opponent() {
    let (_db, repo) = setup_test_db();
    let user = repo.create_user("Hank".to_string()).expect("Create failed");

    for opponent in &["AlphaBot", "AlphaBot", "BetaBot"] {
        let stat = NewGameStat::new(
            *user.id(),
            opponent.to_string(),
            "tictactoe".to_string(),
            "win".to_string(),
            5,
            format!("session_{opponent}"),
        );
        repo.record_game(stat).expect("Record failed");
    }

    let alpha_stats = repo
        .get_stats_by_opponent(*user.id(), "AlphaBot")
        .expect("Stats failed");
    assert_eq!(alpha_stats.len(), 2);

    let beta_stats = repo
        .get_stats_by_opponent(*user.id(), "BetaBot")
        .expect("Stats failed");
    assert_eq!(beta_stats.len(), 1);

    let none_stats = repo
        .get_stats_by_opponent(*user.id(), "GammaBot")
        .expect("Stats failed");
    assert!(none_stats.is_empty());
}

#[test]
fn test_game_outcome_round_trip() {
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
