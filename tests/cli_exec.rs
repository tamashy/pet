use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "plain"
  command = "echo plain output"
  tag = ["demo"]

[[snippets]]
  description = "fails"
  command = "exit 3"

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

#[test]
fn exec_runs_the_selected_command_and_shows_it_first() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("exec")
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("> echo plain output\n")
                .and(predicate::str::contains("plain output")),
        );
}

#[test]
fn exec_silent_suppresses_the_command_echo() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["exec", "-s"])
        .assert()
        .success()
        .stdout("plain output\n");
}

#[test]
fn exec_propagates_nonzero_exit_status() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 2p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("exec")
        .assert()
        .failure()
        .code(1);
}

#[test]
fn exec_on_empty_selection_does_nothing() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "false");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("exec")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn exec_tag_flag_filters_before_selection() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["exec", "-t", "demo"])
        .assert()
        .success()
        .stdout("> echo plain output\nplain output\n");
}

#[test]
fn exec_on_single_selection_with_params_attempts_the_dialog() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 3p");

    // No real TTY in this test harness, proving the (hardcoded raw=false) param
    // dialog really was attempted here, unlike the param-less cases above.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("exec")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to initialize terminal"));
}
