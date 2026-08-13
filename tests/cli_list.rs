use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn list_prints_snippets_from_a_fresh_config_dir() {
    let config_dir = tempfile::tempdir().unwrap();

    // First invocation bootstraps config.toml + empty snippet.toml.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let snippet_file = config_dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        "[[snippets]]\n  description = \"greet\"\n  command = \"echo hi\"\n  tag = [\"demo\"]\n",
    )
    .unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("greet")
                .and(predicate::str::contains("echo hi"))
                .and(predicate::str::contains("demo")),
        );
}

#[test]
fn list_tags_filters_snippets_without_matching_tag() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    pet::config::Config::load(&config_path).unwrap();

    let snippet_file = config_dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        r#"
[[snippets]]
  description = "tagged"
  command = "echo tagged"
  tag = ["keep"]

[[snippets]]
  description = "untagged"
  command = "echo untagged"
"#,
    )
    .unwrap();

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "-t", "keep"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tagged").and(predicate::str::contains("untagged").not()));
}

#[test]
fn list_filter_matches_description_or_command_and_combines_with_tag() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    pet::config::Config::load(&config_path).unwrap();

    let snippet_file = config_dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        r#"
[[snippets]]
  description = "compress a directory"
  command = "tar -czf out.tar.gz ."
  tag = ["files"]

[[snippets]]
  description = "list running containers"
  command = "docker ps"
  tag = ["net"]

[[snippets]]
  description = "ping a host"
  command = "ping 8.8.8.8"
"#,
    )
    .unwrap();

    // Matches by description text alone.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "--oneline", "-f", "directory"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("compress a directory")
                .and(predicate::str::contains("docker ps").not())
                .and(predicate::str::contains("ping a host").not()),
        );

    // Matches by command text alone, case-insensitively.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "--oneline", "-f", "DOCKER"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("list running containers")
                .and(predicate::str::contains("compress a directory").not()),
        );

    // -f and -t combine (AND, not OR): "containers" only survives if it also
    // carries the "net" tag.
    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .args(["list", "--oneline", "-f", "containers", "-t", "files"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn version_prints_version_string() {
    Command::cargo_bin("pet")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("pet version"));
}
