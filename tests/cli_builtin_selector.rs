use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "greet"
  command = "echo hi"
"#;

fn setup_with_builtin_selector(config_dir: &std::path::Path) {
    pet::config::Config::load(&config_dir.join("config.toml")).unwrap();
    std::fs::write(config_dir.join("snippet.toml"), FIXTURE_SNIPPETS).unwrap();

    let config_path = config_dir.join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with("selectcmd = ") {
                "selectcmd = \"builtin\"".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, updated).unwrap();
}

// No real TTY in this test harness, so the picker fails to initialize a
// terminal — that failure is itself proof `selectcmd = "builtin"` really did
// route to the native picker end-to-end (config -> search -> selector ->
// picker), rather than silently falling through to spawning "builtin" as an
// external command (which would fail differently, with a "failed to launch
// selector command" error instead).

#[test]
fn search_with_builtin_selectcmd_attempts_the_native_picker() {
    let config_dir = tempfile::tempdir().unwrap();
    setup_with_builtin_selector(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal").or(predicate::str::contains("raw mode")));
}

#[test]
fn delete_with_builtin_selectcmd_attempts_the_native_picker() {
    let config_dir = tempfile::tempdir().unwrap();
    setup_with_builtin_selector(config_dir.path());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("delete")
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal").or(predicate::str::contains("raw mode")));
}

#[test]
fn edit_with_snippetdirs_and_builtin_selectcmd_attempts_the_native_file_picker() {
    // select_file (edit's multi-dir file picker) is a separate code path from
    // select_snippets — only exercised when snippetdirs is non-empty — so cover
    // it explicitly rather than assuming the shared run_selectcmd branch behaves
    // the same for both callers.
    let config_dir = tempfile::tempdir().unwrap();
    setup_with_builtin_selector(config_dir.path());

    let extra_dir = config_dir.path().join("extra");
    std::fs::create_dir(&extra_dir).unwrap();
    std::fs::write(
        extra_dir.join("side.toml"),
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
        .arg("edit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal").or(predicate::str::contains("raw mode")));
}

#[test]
fn builtin_selectcmd_on_empty_snippet_file_succeeds_without_opening_a_terminal() {
    let config_dir = tempfile::tempdir().unwrap();
    pet::config::Config::load(&config_dir.path().join("config.toml")).unwrap();
    let config_path = config_dir.path().join("config.toml");
    let contents = std::fs::read_to_string(&config_path).unwrap();
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with("selectcmd = ") {
                "selectcmd = \"builtin\"".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&config_path, updated).unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("");
}
