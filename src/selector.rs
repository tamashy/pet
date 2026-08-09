use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::config::GeneralConfig;
use crate::format::render_template;
use crate::snippet::SnippetInfo;

#[derive(Debug, Clone, Default)]
pub struct SelectOptions {
    pub query: Option<String>,
    /// Force-color the description/tags in the text sent to the selector (e.g.
    /// fzf's `--ansi`), regardless of the `color` config — mirrors Go pet's
    /// per-command `--color` flag, which ORs with `general.color`.
    pub color: bool,
}

/// Format every snippet via `general.format`, run the result through the configured
/// selector, and map each selected line back to its stored (unflattened) command.
/// Shared by `search`/`exec`/`clip`.
pub fn select_commands(
    general: &GeneralConfig,
    snippets: &[SnippetInfo],
    opts: &SelectOptions,
) -> Result<Vec<String>> {
    let color = general.color || opts.color;
    let mut lookup: HashMap<String, String> = HashMap::new();
    let mut items = Vec::with_capacity(snippets.len());

    for s in snippets {
        let text = render_template(&general.format, &s.description, &s.command, &s.tag, color);
        items.push(text.clone());
        // Last-write-wins on duplicate display text, matching Go pet's own
        // map[string]SnippetInfo lookup (built from the identical formatted text).
        // Unlike Go, we key the lookup off the *same* (possibly colored) text
        // that's actually sent to the selector — Go's version builds the map key
        // from the uncolored text but sends the colored text to fzf, so a
        // selection made while `color` is enabled can fail to map back to its
        // snippet there.
        lookup.insert(text, s.command.clone());
    }

    let selected_lines = run_selectcmd(general, &items, opts)?;
    Ok(selected_lines
        .into_iter()
        .filter_map(|line| lookup.get(&line).cloned())
        .collect())
}

/// Pick which snippet *file* to edit when `snippetdirs` is configured, using Go
/// pet's `selectFile` display format — `[description]: command #tag1 #tag2` — which
/// is hardcoded and distinct from the general `format` config template used by
/// `select_commands`. Returns `None` if nothing was selected.
pub fn select_file(
    general: &GeneralConfig,
    snippets: &[SnippetInfo],
    opts: &SelectOptions,
) -> Result<Option<PathBuf>> {
    let mut lookup: HashMap<String, PathBuf> = HashMap::new();
    let mut items = Vec::with_capacity(snippets.len());

    for s in snippets {
        let command = s.command.replace('\n', "\\n");
        let mut text = format!("[{}]: {command}", s.description);
        for tag in &s.tag {
            text.push_str(&format!(" #{tag}"));
        }
        items.push(text.clone());
        lookup.insert(text, s.filename.clone());
    }

    let selected_lines = run_selectcmd(general, &items, opts)?;
    Ok(selected_lines
        .into_iter()
        .next()
        .and_then(|line| lookup.get(&line).cloned()))
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
        let mut payload = items.join("\n");
        payload.push('\n');
        // The selector closing stdin before reading everything is normal (`head -1`
        // stops after one line; real fzf can exit as soon as the user picks), not a
        // failure — only bubble up write errors that aren't that.
        if let Err(err) = stdin.write_all(payload.as_bytes())
            && err.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(err).context("failed to write items to selector command");
        }
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
    fn select_commands_with_color_still_maps_back_correctly() {
        // The colored display text (sent to `cat`, our stand-in selector) must be
        // the same text used as the lookup key, or the selection can't be mapped
        // back to its command — see the doc comment on the `color` field.
        let general = GeneralConfig {
            selectcmd: "cat".to_string(),
            color: true,
            ..GeneralConfig::default()
        };
        let snippets = vec![SnippetInfo {
            filename: Default::default(),
            description: "greet".to_string(),
            command: "echo hi".to_string(),
            tag: vec!["demo".to_string()],
            output: String::new(),
        }];

        let result = select_commands(&general, &snippets, &SelectOptions::default()).unwrap();
        assert_eq!(result, vec!["echo hi".to_string()]);
    }

    #[test]
    fn select_commands_per_command_color_flag_ors_with_general_color() {
        let general = GeneralConfig {
            selectcmd: "cat".to_string(),
            color: false,
            ..GeneralConfig::default()
        };
        let snippets = vec![SnippetInfo {
            filename: Default::default(),
            description: "greet".to_string(),
            command: "echo hi".to_string(),
            tag: vec![],
            output: String::new(),
        }];

        let opts = SelectOptions {
            color: true,
            ..SelectOptions::default()
        };
        let result = select_commands(&general, &snippets, &opts).unwrap();
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

    #[test]
    fn select_file_uses_hardcoded_display_format_and_returns_origin_path() {
        let general = GeneralConfig {
            selectcmd: "cat".to_string(),
            ..GeneralConfig::default()
        };
        let snippets = vec![SnippetInfo {
            filename: PathBuf::from("/snippets/a.toml"),
            description: "greet".to_string(),
            command: "echo hi".to_string(),
            tag: vec!["demo".to_string()],
            output: String::new(),
        }];

        let result = select_file(&general, &snippets, &SelectOptions::default()).unwrap();
        assert_eq!(result, Some(PathBuf::from("/snippets/a.toml")));
    }

    #[test]
    fn select_file_on_no_selection_returns_none() {
        let general = GeneralConfig {
            selectcmd: "false".to_string(),
            ..GeneralConfig::default()
        };
        let snippets = vec![SnippetInfo {
            filename: PathBuf::from("/snippets/a.toml"),
            description: "greet".to_string(),
            command: "echo hi".to_string(),
            tag: vec![],
            output: String::new(),
        }];

        let result = select_file(&general, &snippets, &SelectOptions::default()).unwrap();
        assert_eq!(result, None);
    }
}
