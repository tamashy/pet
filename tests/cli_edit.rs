use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

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

fn set_config_field(config_path: &Path, field: &str, value: &str) {
    let contents = std::fs::read_to_string(config_path).unwrap();
    let updated = contents
        .lines()
        .map(|line| {
            if line.starts_with(&format!("{field} = ")) {
                format!("{field} = \"{value}\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(config_path, updated).unwrap();
}

#[test]
fn edit_with_no_snippetdirs_opens_the_main_snippet_file_directly() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    pet::config::Config::load(&config_path).unwrap();

    let fake_editor = config_dir.path().join("fake-editor.sh");
    write_fake_editor(&fake_editor);
    set_config_field(&config_path, "editor", &fake_editor.to_string_lossy());

    Command::cargo_bin("pet")
        .unwrap()
        .env("PET_CONFIG_DIR", config_dir.path())
        .arg("edit")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("FAKE_EDITOR_CALLED")
                .and(predicate::str::contains("+0"))
                .and(predicate::str::contains("snippet.toml")),
        );
}

#[test]
fn edit_with_snippetdirs_picks_a_file_via_the_selector() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    pet::config::Config::load(&config_path).unwrap();

    std::fs::write(
        config_dir.path().join("snippet.toml"),
        "[[snippets]]\ndescription = \"main\"\ncommand = \"echo main\"\n",
    )
    .unwrap();

    let extra_dir = config_dir.path().join("extra");
    std::fs::create_dir(&extra_dir).unwrap();
    std::fs::write(
        extra_dir.join("more.toml"),
        "[[snippets]]\ndescription = \"extra\"\ncommand = \"echo extra\"\n",
    )
    .unwrap();

    let fake_editor = config_dir.path().join("fake-editor.sh");
    write_fake_editor(&fake_editor);
    set_config_field(&config_path, "editor", &fake_editor.to_string_lossy());
    set_config_field(&config_path, "selectcmd", "grep extra");

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
        .success()
        .stdout(
            predicate::str::contains("FAKE_EDITOR_CALLED")
                .and(predicate::str::contains("extra/more.toml")),
        );
}

#[test]
fn edit_with_snippetdirs_and_no_selection_fails_cleanly() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    pet::config::Config::load(&config_path).unwrap();

    let extra_dir = config_dir.path().join("extra");
    std::fs::create_dir(&extra_dir).unwrap();
    std::fs::write(
        extra_dir.join("more.toml"),
        "[[snippets]]\ndescription = \"extra\"\ncommand = \"echo extra\"\n",
    )
    .unwrap();

    set_config_field(&config_path, "selectcmd", "false");
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
        .stderr(predicate::str::contains("no snippet file selected"));
}
