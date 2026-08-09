use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "one"
  command = "echo one"
  tag = ["demo"]

[[snippets]]
  description = "two"
  command = "echo two"

[[snippets]]
  description = "three"
  command = "echo three"
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
fn delete_removes_only_the_selected_snippet() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "sed -n 2p");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted: two"));

    let contents = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(contents.contains("\"one\""));
    assert!(!contents.contains("\"two\""));
    assert!(contents.contains("\"three\""));
}

#[test]
fn delete_on_empty_selection_leaves_the_file_untouched() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "false");

    let before = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .success()
        .stdout("");

    let after = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert_eq!(before, after);
}

#[test]
fn delete_tag_flag_filters_before_selection() {
    let config_dir = tempfile::tempdir().unwrap();
    // "cat" selects every line handed to it, but the tag filter should narrow
    // that set down to "one" before the selector ever sees "two"/"three".
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["delete", "-t", "demo"])
        .assert()
        .success()
        .stdout("Deleted: one\n");

    let contents = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(!contents.contains("\"one\""));
    assert!(contents.contains("\"two\""));
    assert!(contents.contains("\"three\""));
}

#[test]
fn delete_multi_select_removes_every_selected_snippet() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .success()
        .stdout("Deleted: one\nDeleted: two\nDeleted: three\n");

    let contents = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert_eq!(contents, "");
}

#[test]
fn delete_still_matches_the_selection_when_the_selector_strips_ansi_codes() {
    let config_dir = tempfile::tempdir().unwrap();
    // Real `fzf --ansi` (the default selectcmd) strips color escape codes from the
    // line it hands back on selection. Fresh configs default to `color = true`, so
    // reproduce that stripping here with `perl` instead of depending on a real fzf
    // binary being present in CI. Regression test for the exact bug report: `pet
    // delete -t demo` picked a snippet in the list but nothing actually got
    // deleted, because the lookup was keyed on the colored text that never comes
    // back once a real ANSI-aware selector strips it.
    setup(
        config_dir.path(),
        r#"perl -pe 's/\\e\\[[0-9;]*m//g' | sed -n 1p"#,
    );

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["delete", "-t", "demo"])
        .assert()
        .success()
        .stdout("Deleted: one\n");

    let contents = std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(!contents.contains("\"one\""));
}

#[test]
fn delete_empties_a_snippetdir_file_without_leaving_stale_content() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    let extra_dir = config_dir.path().join("extra");
    std::fs::create_dir(&extra_dir).unwrap();
    let extra_file = extra_dir.join("side.toml");
    std::fs::write(
        &extra_file,
        "[[snippets]]\n  description = \"only\"\n  command = \"echo only\"\n",
    )
    .unwrap();

    let config_path = config_dir.path().join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents.replace(
        "snippetdirs = []",
        &format!("snippetdirs = [\"{}\"]", extra_dir.to_string_lossy()),
    );
    std::fs::write(&config_path, updated).unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .success()
        .stdout(predicate::str::contains("Deleted: only"));

    let side_contents = std::fs::read_to_string(&extra_file).unwrap();
    assert_eq!(side_contents, "");
}
