//! Tests for AgentLibrary config directory scanning.

use std::fs;
use tempfile::TempDir;

use strictly_games::AgentLibrary;

/// Creates a temporary directory with one or more agent TOML files.
fn make_agent_toml(dir: &TempDir, filename: &str, name: &str) {
    let content = format!(
        r#"name = "{name}"
server_command = ["echo", "test"]
llm_provider = "anthropic"
llm_model = "claude-3-5-haiku-20241022"
llm_max_tokens = 50
"#
    );
    fs::write(dir.path().join(filename), content).expect("Failed to write TOML");
}

#[test]
fn test_scan_loads_valid_configs() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "agent_a.toml", "AgentA");
    make_agent_toml(&dir, "agent_b.toml", "AgentB");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    assert_eq!(library.len(), 2);
}

#[test]
fn test_scan_sorted_by_name() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "zzz.toml", "Zebra");
    make_agent_toml(&dir, "aaa.toml", "Apple");
    make_agent_toml(&dir, "mmm.toml", "Mango");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    let names: Vec<&str> = library.agents().iter().map(|a| a.name().as_str()).collect();
    assert_eq!(names, ["Apple", "Mango", "Zebra"]);
}

#[test]
fn test_scan_skips_non_toml_files() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "valid.toml", "ValidAgent");
    fs::write(dir.path().join("notes.txt"), "not a config").expect("Write failed");
    fs::write(dir.path().join("config.json"), "{}").expect("Write failed");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    assert_eq!(library.len(), 1);
    assert_eq!(library.agents()[0].name(), "ValidAgent");
}

#[test]
fn test_scan_skips_invalid_toml() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "good.toml", "GoodAgent");
    fs::write(dir.path().join("bad.toml"), "this is not valid toml !!!@@@").expect("Write failed");

    let library = AgentLibrary::scan(dir.path()).expect("Scan should succeed despite bad file");
    assert_eq!(library.len(), 1);
    assert_eq!(library.agents()[0].name(), "GoodAgent");
}

#[test]
fn test_scan_empty_directory_fails() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let result = AgentLibrary::scan(dir.path());
    assert!(result.is_err(), "Empty directory should return error");
}

#[test]
fn test_scan_nonexistent_directory_fails() {
    let result = AgentLibrary::scan("/this/path/does/not/exist/at/all");
    assert!(result.is_err());
}

#[test]
fn test_scan_file_path_fails() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let file_path = dir.path().join("a_file.toml");
    fs::write(&file_path, "name = \"X\"\nserver_command = [\"echo\"]\n").expect("Write failed");

    let result = AgentLibrary::scan(&file_path);
    assert!(result.is_err(), "File path should return error");
}

#[test]
fn test_get_by_name_found() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "alpha.toml", "Alpha");
    make_agent_toml(&dir, "beta.toml", "Beta");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    let found = library.get_by_name("Alpha");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name(), "Alpha");
}

#[test]
fn test_get_by_name_not_found() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "agent.toml", "SomeAgent");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    assert!(library.get_by_name("NoSuchAgent").is_none());
}

#[test]
fn test_is_empty_false_when_loaded() {
    let dir = TempDir::new().expect("Failed to create temp dir");
    make_agent_toml(&dir, "agent.toml", "SomeAgent");

    let library = AgentLibrary::scan(dir.path()).expect("Scan failed");
    assert!(!library.is_empty());
}

#[test]
fn test_scan_examples_directory() {
    // examples/ directory has agent configs — verify the directory scan works end-to-end.
    let library = AgentLibrary::scan("examples").expect("Scan of examples/ failed");
    assert!(
        !library.is_empty(),
        "examples/ should contain agent configs"
    );
}
