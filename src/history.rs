use std::env;
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

/// Find the most recent shell command in the user's history file, for `pet new
/// --last`. Skips trailing entries that are themselves `pet` invocations, since
/// shells with immediate history append (zsh's `INC_APPEND_HISTORY`, or bash with
/// `history -a` in `PROMPT_COMMAND`) may have already recorded the running
/// `pet new --last` command as the newest entry by the time we read the file.
pub fn last_command() -> Result<String> {
    let path = history_file_path()?;
    let contents = std::fs::read_to_string(&path).map_err(|source| {
        anyhow!(
            "failed to read shell history file {}: {source}",
            path.display()
        )
    })?;

    parse_history(&contents, &shell_name())
        .into_iter()
        .rev()
        .find(|c| !is_pet_invocation(c))
        .ok_or_else(|| anyhow!("no previous command found in {}", path.display()))
}

fn shell_name() -> String {
    env::var("SHELL")
        .ok()
        .and_then(|s| {
            PathBuf::from(s)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

fn history_file_path() -> Result<PathBuf> {
    if let Ok(histfile) = env::var("HISTFILE")
        && !histfile.is_empty()
    {
        return Ok(PathBuf::from(histfile));
    }

    let home = env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| anyhow!("failed to determine home directory ($HOME not set)"))?;

    let path = match shell_name().as_str() {
        "zsh" => home.join(".zsh_history"),
        "fish" => home.join(".local/share/fish/fish_history"),
        _ => home.join(".bash_history"),
    };

    if !path.exists() {
        bail!(
            "couldn't find a shell history file at {} (set $HISTFILE, or check that your shell writes history immediately — zsh's INC_APPEND_HISTORY, or bash's `history -a` in PROMPT_COMMAND)",
            path.display()
        );
    }
    Ok(path)
}

/// Parse a history file's contents into an ordered list of commands (oldest
/// first), stripping shell-specific framing.
fn parse_history(contents: &str, shell: &str) -> Vec<String> {
    let lines: Vec<String> = match shell {
        "fish" => parse_fish_history(contents),
        "zsh" => contents.lines().map(strip_zsh_extended_prefix).collect(),
        _ => contents.lines().map(str::to_string).collect(),
    };

    lines
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extended zsh history entries look like `: <start>:<duration>;<command>`.
/// Plain entries (`setopt EXTENDED_HISTORY` off) pass through unchanged.
fn strip_zsh_extended_prefix(line: &str) -> String {
    line.strip_prefix(':')
        .and_then(|rest| rest.find(';').map(|semi| rest[semi + 1..].to_string()))
        .unwrap_or_else(|| line.to_string())
}

/// Fish history is YAML-ish: `- cmd: <command>` lines interleaved with metadata
/// (`  when: ...`, `  paths: ...`) that we don't need.
fn parse_fish_history(contents: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|line| line.strip_prefix("- cmd: "))
        .map(str::to_string)
        .collect()
}

fn is_pet_invocation(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .map(|first| PathBuf::from(first).file_name().is_some_and(|f| f == "pet"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_bash_history_returns_lines_in_order() {
        let contents = "echo one\necho two\n";
        assert_eq!(
            parse_history(contents, "bash"),
            vec!["echo one", "echo two"]
        );
    }

    #[test]
    fn zsh_extended_history_strips_timestamp_prefix() {
        let contents = ": 1700000000:0;echo one\n: 1700000001:0;echo two\n";
        assert_eq!(parse_history(contents, "zsh"), vec!["echo one", "echo two"]);
    }

    #[test]
    fn zsh_plain_history_passes_through() {
        let contents = "echo one\necho two\n";
        assert_eq!(parse_history(contents, "zsh"), vec!["echo one", "echo two"]);
    }

    #[test]
    fn fish_history_extracts_cmd_lines_only() {
        let contents = "- cmd: echo one\n  when: 1700000000\n- cmd: echo two\n  when: 1700000001\n";
        assert_eq!(
            parse_history(contents, "fish"),
            vec!["echo one", "echo two"]
        );
    }

    #[test]
    fn blank_lines_are_dropped() {
        let contents = "echo one\n\n\necho two\n";
        assert_eq!(
            parse_history(contents, "bash"),
            vec!["echo one", "echo two"]
        );
    }

    #[test]
    fn pet_invocation_is_detected_by_first_word() {
        assert!(is_pet_invocation("pet new --last"));
        assert!(is_pet_invocation("/usr/local/bin/pet search"));
        assert!(!is_pet_invocation("echo pet new --last"));
        assert!(!is_pet_invocation("petstore list"));
    }
}
