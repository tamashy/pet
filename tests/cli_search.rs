use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

const FIXTURE_SNIPPETS: &str = r#"
[[snippets]]
  description = "Show expiration date of SSL certificate"
  command = "echo | openssl s_client -connect example.com:443"
  tag = ["ssl", "net"]

[[snippets]]
  description = "List big files"
  command = "find . -size +10M"
  tag = ["fs"]

[[snippets]]
  description = "Multiline"
  command = """
echo one
echo two"""
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

#[test]
fn search_prints_the_first_selected_commands_raw_text() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("echo | openssl s_client -connect example.com:443");
}

#[test]
fn search_raw_flag_behaves_the_same_as_default_for_now() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -1");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "--raw"])
        .assert()
        .success()
        .stdout("echo | openssl s_client -connect example.com:443");
}

#[test]
fn search_joins_multiple_selections_with_the_default_delimiter() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout(predicate::eq(
            "echo | openssl s_client -connect example.com:443; find . -size +10M; echo one\necho two",
        ));
}

#[test]
fn search_delimiter_flag_overrides_the_join_separator() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "head -2");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-d", " | "])
        .assert()
        .success()
        .stdout("echo | openssl s_client -connect example.com:443 | find . -size +10M");
}

#[test]
fn search_tag_flag_filters_before_selection() {
    let config_dir = tempfile::tempdir().unwrap();
    // cat echoes back everything it's given, so this proves only the tagged
    // snippet's display line ever reached the selector.
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-t", "fs"])
        .assert()
        .success()
        .stdout("find . -size +10M");
}

#[test]
fn search_filter_flag_matches_description_or_command_before_selection() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    // "openssl" only appears in the SSL snippet's command, not its description.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-f", "openssl"])
        .assert()
        .success()
        .stdout("echo | openssl s_client -connect example.com:443");
}

#[test]
fn search_filter_and_tag_flags_combine() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    // "big" matches the "fs"-tagged snippet's description; requiring tag "demo"
    // on top of that leaves nothing.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-f", "big", "-t", "demo"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn search_tag_with_no_matches_selects_nothing() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "cat");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-t", "does-not-exist"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn search_cancelled_selection_prints_nothing_and_still_succeeds() {
    let config_dir = tempfile::tempdir().unwrap();
    // `false` exits non-zero without reading stdin, exactly like fzf on Esc/Ctrl-C.
    setup(config_dir.path(), "false");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn search_multiline_command_keeps_real_newlines_in_output() {
    let config_dir = tempfile::tempdir().unwrap();
    setup(config_dir.path(), "grep Multiline");

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("search")
        .assert()
        .success()
        .stdout("echo one\necho two");
}

#[test]
fn search_query_flag_is_forwarded_to_the_selector_command() {
    let config_dir = tempfile::tempdir().unwrap();
    let fake_selector = config_dir.path().join("fake-selector.sh");
    std::fs::write(
        &fake_selector,
        "#!/bin/sh\necho \"ARGS: $*\" >&2\nhead -1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake_selector).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_selector, perms).unwrap();
    }
    setup(config_dir.path(), &fake_selector.to_string_lossy());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["search", "-q", "it's a query"])
        .assert()
        .success()
        // The fake selector runs through `sh -c`, so by the time it sees argv the
        // shell has already undone our quoting — this proves the value survives
        // that round trip as a single argument, embedded quote and all.
        .stderr(predicate::str::contains("ARGS: --query it's a query"));
}
