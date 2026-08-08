use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "greet"
  command = "echo Hello, <name=world>!"

[[snippets]]
  description = "no params here"
  command = "echo hi"
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

// assert_cmd spawns without a real TTY, so we can't drive the interactive dialog
// end-to-end here (that's covered by manual pty-driven testing). What these tests
// prove instead is the *gating* logic that decides whether to even attempt it —
// matching Go pet's cmd/util.go `filter()`: only a single non-raw selection whose
// command actually contains a param opens the dialog; every other case must return
// the stored command untouched without ever trying to initialize a terminal.

#[test]
fn raw_flag_skips_the_dialog_and_prints_placeholder_unsubstituted() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "--raw"])
        .assert()
        .success()
        .stdout("echo Hello, <name=world>!");
}

#[test]
fn multi_select_skips_the_dialog_even_without_raw() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("echo Hello, <name=world>!; echo hi");
}

#[test]
fn param_less_single_selection_skips_the_dialog() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 2p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("echo hi");
}

#[test]
fn single_selection_with_params_attempts_the_dialog() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    // No real TTY in this test harness, so the dialog fails to initialize — that
    // failure is itself proof the gating logic *did* try to open it here, unlike
    // the three cases above which succeed without ever attempting to.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to initialize terminal"));
}
