use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "plain"
  command = "echo plain output"

[[snippets]]
  description = "greet"
  command = "echo Hello, <name=world>!"
"#;

fn setup(config_dir: &Path, selectcmd: &str) {
    pet::config::Config::load(&config_dir.join("config.toml")).unwrap();
    std::fs::write(config_dir.join("snippet.toml"), FIXTURE_SNIPPETS).unwrap();

    let config_path = config_dir.join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with("selectcmd = ") {
                format!("selectcmd = \"{selectcmd}\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, updated).unwrap();
}

// clip's happy path needs a real clipboard/display server, which headless Linux CI
// doesn't have (arboard::Clipboard::new() itself fails there) — those are manually
// verified instead (see the M5 PR description) and marked #[ignore] here so they're
// still runnable on demand (`cargo test -- --ignored`) without breaking CI. What
// *can* run in CI is anything that returns before ever touching the clipboard.

#[test]
fn clip_on_empty_selection_never_touches_the_clipboard() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "false");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("clip")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn clip_single_selection_with_params_attempts_the_dialog_before_clipboard() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 2p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("clip")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to initialize terminal"));
}

#[test]
#[ignore = "needs a real clipboard/display server; run manually with -- --ignored"]
fn clip_copies_the_selected_command() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("clip")
        .assert()
        .success();

    let mut clipboard = arboard::Clipboard::new().unwrap();
    assert_eq!(clipboard.get_text().unwrap(), "echo plain output");
}

#[test]
#[ignore = "needs a real clipboard/display server; run manually with -- --ignored"]
fn clip_command_flag_prints_before_copying() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["clip", "--command"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Command:").and(predicate::str::contains("echo plain output")),
        );
}
