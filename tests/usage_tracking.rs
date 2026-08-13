use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "first"
  command = "echo first"

[[snippets]]
  description = "second"
  command = "echo second"

[[snippets]]
  description = "third"
  command = "echo third"
"#;

fn setup(config_dir: &Path, selectcmd: &str) {
    pet::config::Config::load(&config_dir.join("config.toml")).unwrap();
    std::fs::write(config_dir.join("snippet.toml"), FIXTURE_SNIPPETS).unwrap();
    set_config_field(config_dir, "selectcmd", selectcmd);
}

fn set_config_field(config_dir: &Path, field: &str, value: &str) {
    let config_path = config_dir.join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let prefix = format!("{field} = ");
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with(&prefix) {
                format!("{field} = \"{value}\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, updated).unwrap();
}

fn load_usage(config_dir: &Path) -> pet::usage::UsageStats {
    pet::usage::UsageStats::load(&config_dir.join("usage.toml")).unwrap()
}

#[test]
fn search_records_a_use_of_the_selected_snippet() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 1p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success();

    let usage = load_usage(config_dir.path());
    assert_eq!(usage.score("first").count, 1);
    assert_eq!(usage.score("second").count, 0);
}

#[test]
fn exec_records_a_use_of_the_selected_snippet() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 2p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["exec", "-s"])
        .assert()
        .success();

    let usage = load_usage(config_dir.path());
    assert_eq!(usage.score("second").count, 1);
}

#[test]
fn cancelled_selection_does_not_record_a_use() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "false");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success();

    assert!(load_usage(config_dir.path()).entries.is_empty());
}

#[test]
fn repeated_use_increments_the_count() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 1p");

    for _ in 0..3 {
        Command::cargo_bin("pet")
            .unwrap()
            .env("PET_CONFIG_DIR", config_dir.path())
            .arg("search")
            .assert()
            .success();
    }

    assert_eq!(load_usage(config_dir.path()).score("first").count, 3);
}

#[test]
fn delete_removes_the_snippets_usage_entry() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 1p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success();
    assert_eq!(load_usage(config_dir.path()).score("first").count, 1);

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .success();

    assert!(!load_usage(config_dir.path()).entries.contains_key("first"));
}

#[test]
fn sortby_usage_ranks_the_most_used_snippet_first() {
    let config_dir = tempfile::tempdir().unwrap();
    // Insertion order is first/second/third, and sortby is still the default
    // ("recency", a no-op) for these two calls, so "sed -n 3p" reliably selects
    // "third" both times, ahead of setting sortby=usage below.
    setup(config_dir.path(), "sed -n 3p");

    for _ in 0..2 {
        Command::cargo_bin("pet")
            .unwrap()
            .env("PET_CONFIG_DIR", config_dir.path())
            .arg("search")
            .assert()
            .success();
    }

    set_config_field(config_dir.path(), "sortby", "usage");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "--oneline"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("third"));
}
