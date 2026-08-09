use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "greet"
  command = "echo hi"
  tag = ["demo"]
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

// list/list --oneline print directly to our own stdout, so their coloring must
// respect TTY auto-detection — assert_cmd's captured stdout isn't a TTY, so no
// ANSI codes should appear at all here (regardless of the `color` config, which
// only affects the text fed to a selector, never list's own output).

#[test]
fn list_output_has_no_ansi_codes_when_not_a_tty() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b[").not());
}

#[test]
fn list_oneline_output_has_no_ansi_codes_when_not_a_tty() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "--oneline"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b[").not());
}

// search/exec/clip's --color (and the general.color config) only affects the text
// piped to the selector, not pet's own final output — verify by using a selector
// stand-in that echoes its stdin to a file we can inspect directly.

#[test]
fn search_color_flag_colors_the_selector_text_but_not_the_final_output() {
    let config_dir = tempfile::tempdir().unwrap();
    let seen_file = config_dir.path().join("seen.txt");
    setup(
        config_dir.path(),
        &format!("tee {}", seen_file.to_string_lossy()),
    );

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "--color"])
        .assert()
        .success()
        .stdout(predicate::eq("echo hi").and(predicate::str::contains("\x1b[").not()));

    let seen = std::fs::read_to_string(&seen_file).unwrap();
    assert!(
        seen.contains("\x1b["),
        "expected ANSI codes in the text sent to the selector, got: {seen:?}"
    );
}

#[test]
fn search_without_color_sends_plain_text_to_the_selector() {
    let config_dir = tempfile::tempdir().unwrap();
    let seen_file = config_dir.path().join("seen.txt");
    setup(
        config_dir.path(),
        &format!("tee {}", seen_file.to_string_lossy()),
    );

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success();

    let seen = std::fs::read_to_string(&seen_file).unwrap();
    assert!(
        !seen.contains("\x1b["),
        "expected no ANSI codes, got: {seen:?}"
    );
}

#[test]
fn general_color_config_has_the_same_effect_as_the_flag() {
    let config_dir = tempfile::tempdir().unwrap();
    let seen_file = config_dir.path().join("seen.txt");
    setup(
        config_dir.path(),
        &format!("tee {}", seen_file.to_string_lossy()),
    );

    let config_path = config_dir.path().join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents.replace("color = false", "color = true");
    std::fs::write(&config_path, updated).unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("echo hi");

    let seen = std::fs::read_to_string(&seen_file).unwrap();
    assert!(seen.contains("\x1b["), "expected ANSI codes, got: {seen:?}");
}
