use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn bootstrap(config_dir: &Path) {
    pet::config::Config::load(&config_dir.join("config.toml")).unwrap();
}

fn write_fake_editor(path: &Path) {
    std::fs::write(path, "#!/bin/sh\necho \"FAKE_EDITOR_CALLED: $*\"\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).unwrap();
    }
}

fn set_editor(config_dir: &Path, editor_path: &Path) {
    let config_path = config_dir.join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with("editor = ") {
                format!("editor = \"{}\"", editor_path.display())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, updated).unwrap();
}

#[test]
fn new_with_positional_command_prompts_only_for_description() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "echo", "hello", "world"])
        .write_stdin("My greeting\n")
        .assert()
        .success();

    let snippet_toml =
        std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(snippet_toml.contains("echo hello world"));
    assert!(snippet_toml.contains("My greeting"));
}

#[test]
fn new_interactive_with_tag_prompt_splits_tags_on_whitespace() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "-t"])
        .write_stdin("echo second\nSecond description\ndemo test\n")
        .assert()
        .success();

    let snippet_toml =
        std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(snippet_toml.contains("echo second"));
    assert!(snippet_toml.contains("\"demo\""));
    assert!(snippet_toml.contains("\"test\""));
}

#[test]
fn new_rejects_duplicate_description() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "echo", "one"])
        .write_stdin("dup desc\n")
        .assert()
        .success();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "echo", "two"])
        .write_stdin("dup desc\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn new_multiline_collects_lines_until_double_blank() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "-m"])
        .write_stdin("echo one\necho two\n\n\nMultiline desc\n")
        .assert()
        .success();

    let snippet_toml =
        std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(snippet_toml.contains("echo one\necho two"));
}

#[test]
fn new_editor_mode_appends_empty_snippet_and_opens_editor() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    let fake_editor = config_dir.path().join("fake-editor.sh");
    write_fake_editor(&fake_editor);
    set_editor(config_dir.path(), &fake_editor);

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["new", "-e"])
        .assert()
        .success()
        .stdout(predicate::str::contains("FAKE_EDITOR_CALLED"));

    let snippet_toml =
        std::fs::read_to_string(config_dir.path().join("snippet.toml")).unwrap();
    assert!(snippet_toml.contains("description = \"\""));
    assert!(snippet_toml.contains("command = \"\""));
}

#[test]
fn configure_opens_config_file_at_line_zero() {
    let config_dir = tempfile::tempdir().unwrap();
    bootstrap(config_dir.path());

    let fake_editor = config_dir.path().join("fake-editor.sh");
    write_fake_editor(&fake_editor);
    set_editor(config_dir.path(), &fake_editor);

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("configure")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("FAKE_EDITOR_CALLED")
                .and(predicate::str::contains("+0"))
                .and(predicate::str::contains("config.toml")),
        );
}
