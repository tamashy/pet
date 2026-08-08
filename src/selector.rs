use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::config::GeneralConfig;
use crate::format::render_template;
use crate::snippet::SnippetInfo;

#[derive(Debug, Clone, Default)]
pub struct SelectOptions {
    pub query: Option<String>,
}

/// Format every snippet via `general.format`, run the result through the configured
/// selector, and map each selected line back to its stored (unflattened) command.
/// Shared by `search`/`exec`/`clip`.
pub fn select_commands(
    general: &GeneralConfig,
    snippets: &[SnippetInfo],
    opts: &SelectOptions,
) -> Result<Vec<String>> {
    let mut lookup: HashMap<String, String> = HashMap::new();
    let mut items = Vec::with_capacity(snippets.len());

    for s in snippets {
        let text = render_template(&general.format, &s.description, &s.command, &s.tag);
        items.push(text.clone());
        // Last-write-wins on duplicate display text, matching Go pet's own
        // map[string]SnippetInfo lookup (built from the identical formatted text).
        lookup.insert(text, s.command.clone());
    }

    let selected_lines = run_selectcmd(general, &items, opts)?;
    Ok(selected_lines
        .into_iter()
        .filter_map(|line| lookup.get(&line).cloned())
        .collect())
}

/// Pipe `items` (one per line) into the configured `selectcmd`, spawned through
/// `general.cmd` (default `sh -c`), and return the selected line(s). Stderr is
/// inherited so the selector's own UI (e.g. fzf drawing to the terminal) is still
/// visible. A non-zero exit (cancel) is treated as "nothing selected", matching Go
/// pet's handling of fzf's Esc/Ctrl-C.
fn run_selectcmd(
    general: &GeneralConfig,
    items: &[String],
    opts: &SelectOptions,
) -> Result<Vec<String>> {
    if items.is_empty() {
        return Ok(vec![]);
    }

    let mut selectcmd = general.selectcmd.clone();
    if let Some(query) = &opts.query {
        selectcmd.push_str(" --query ");
        selectcmd.push_str(&shell_quote(query));
    }

    let mut cmd = if general.cmd.is_empty() {
        let mut c = Command::new("sh");
        c.arg("-c");
        c
    } else {
        let mut c = Command::new(&general.cmd[0]);
        c.args(&general.cmd[1..]);
        c
    };
    cmd.arg(&selectcmd);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to launch selector command: {selectcmd}"))?;

    {
        let mut stdin = child.stdin.take().expect("stdin was piped");
        stdin.write_all(items.join("\n").as_bytes())?;
        stdin.write_all(b"\n")?;
    }

    let output = child
        .wait_with_output()
        .context("failed to read selector output")?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    let selected = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    Ok(selected)
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_wraps_plain_values() {
        assert_eq!(shell_quote("hello"), "'hello'");
    }

    #[test]
    fn shell_quote_escapes_embedded_single_quotes() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn select_commands_maps_display_text_back_to_stored_command() {
        let general = GeneralConfig {
            selectcmd: "cat".to_string(),
            ..GeneralConfig::default()
        };
        let snippets = vec![SnippetInfo {
            filename: Default::default(),
            description: "greet".to_string(),
            command: "echo hi".to_string(),
            tag: vec![],
            output: String::new(),
        }];

        let result = select_commands(&general, &snippets, &SelectOptions::default()).unwrap();
        assert_eq!(result, vec!["echo hi".to_string()]);
    }

    #[test]
    fn select_commands_on_no_snippets_returns_empty_without_spawning() {
        let general = GeneralConfig {
            selectcmd: "this-binary-does-not-exist-anywhere".to_string(),
            ..GeneralConfig::default()
        };
        let result = select_commands(&general, &[], &SelectOptions::default()).unwrap();
        assert!(result.is_empty());
    }
}
