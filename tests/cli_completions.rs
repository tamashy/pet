use assert_cmd::Command;
use predicates::prelude::*;

// completions doesn't touch config/snippets, so it should work even against a
// config dir that's never been initialized — unlike every other subcommand,
// which creates one on first run.
fn uninitialized_config_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn completions_bash_prints_a_bash_completion_script() {
    let config_dir = uninitialized_config_dir();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_pet()"));

    assert!(
        std::fs::read_dir(config_dir.path())
            .unwrap()
            .next()
            .is_none(),
        "completions should not create any files under the config dir"
    );
}

#[test]
fn completions_zsh_prints_a_zsh_completion_script() {
    let config_dir = uninitialized_config_dir();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef pet"));
}

#[test]
fn completions_fish_prints_a_fish_completion_script() {
    let config_dir = uninitialized_config_dir();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -c pet"));
}

#[test]
fn completions_rejects_an_unknown_shell() {
    let config_dir = uninitialized_config_dir();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["completions", "not-a-real-shell"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}
