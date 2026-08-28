use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use dialoguer::Input;
use owo_colors::{OwoColorize, Stream::Stdout};

use crate::config::Config;
use crate::dialog;
use crate::editor;
use crate::error::SnippetError;
use crate::history;
use crate::path::expand_absolute;
use crate::shell::{self, CaptureOutcome};
use crate::snippet::{SnippetInfo, Snippets};

/// How long `-o`/`--capture-output` waits for the command to finish before
/// giving up and continuing without output. Fixed rather than configurable —
/// narrow enough a use case not to need its own config field.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct NewOptions {
    pub command_args: Vec<String>,
    pub prompt_tag: bool,
    pub multiline: bool,
    pub use_editor: bool,
    pub use_last: bool,
    pub capture_output: bool,
}

pub fn run(config: &Config, opts: NewOptions) -> Result<()> {
    let mut snippets = Snippets::load(&config.general, false)?;

    if opts.use_editor {
        let line_count = count_snippet_lines(&config.general)?;
        snippets.snippets.push(SnippetInfo {
            filename: PathBuf::new(),
            description: String::new(),
            command: String::new(),
            tag: vec![],
            output: String::new(),
        });
        snippets.save(&config.general)?;

        let snippet_path = expand_absolute(&config.general.snippetfile)?;
        editor::open(&config.general, &snippet_path, line_count + 3)?;
        return Ok(());
    }

    let command = if opts.use_last {
        let command = history::last_command()?;
        println!(
            "{} {command}",
            "Command>".if_supports_color(Stdout, |t| t.bright_yellow())
        );
        command
    } else if !opts.command_args.is_empty() {
        let command = opts.command_args.join(" ");
        println!(
            "{} {command}",
            "Command>".if_supports_color(Stdout, |t| t.bright_yellow())
        );
        command
    } else if opts.multiline {
        scan_multiline()?
    } else {
        scan("Command> ", false)?
    };

    let captured = resolve_output_capture(opts.capture_output, &command, || {
        shell::capture_stdout(&config.general, &command, CAPTURE_TIMEOUT)
    });
    if let Some(message) = &captured.message {
        println!("{message}");
    }

    let description = scan("Description> ", false)?;

    let tag = if opts.prompt_tag {
        let t = scan("Tag> ", true)?;
        if t.is_empty() {
            vec![]
        } else {
            t.split_whitespace().map(String::from).collect()
        }
    } else {
        vec![]
    };

    if snippets
        .snippets
        .iter()
        .any(|s| s.description == description)
    {
        return Err(SnippetError::DuplicateDescription(description).into());
    }

    snippets.snippets.push(SnippetInfo {
        filename: PathBuf::new(),
        description,
        command,
        tag,
        output: captured.text,
    });
    snippets.save(&config.general)?;

    Ok(())
}

struct CapturedOutput {
    text: String,
    message: Option<String>,
}

/// Decide a new snippet's `output` field for `-o`/`--capture-output`.
/// `run_capture` is injected (production calls `shell::capture_stdout`; tests
/// pass a canned outcome or a panic-if-called closure) so this decision logic
/// — whether to attempt capture at all, and how each outcome maps to the
/// stored text and the message shown to the user — is testable without
/// spawning a real process. Capture failing (in any way) never blocks snippet
/// creation; it just leaves `output` empty.
fn resolve_output_capture(
    capture_output: bool,
    command: &str,
    run_capture: impl FnOnce() -> io::Result<CaptureOutcome>,
) -> CapturedOutput {
    if !capture_output {
        return CapturedOutput {
            text: String::new(),
            message: None,
        };
    }

    if !dialog::extract_params(command).is_empty() {
        return CapturedOutput {
            text: String::new(),
            message: Some(
                "Skipping output capture: command has <param> placeholder(s)".to_string(),
            ),
        };
    }

    match run_capture() {
        Ok(CaptureOutcome::Captured(text)) => CapturedOutput {
            text,
            message: Some("Captured command output.".to_string()),
        },
        Ok(CaptureOutcome::Failed(status)) => CapturedOutput {
            text: String::new(),
            message: Some(format!("Command failed ({status}), output not captured.")),
        },
        Ok(CaptureOutcome::TimedOut) => CapturedOutput {
            text: String::new(),
            message: Some(format!(
                "Command timed out after {}s, output not captured.",
                CAPTURE_TIMEOUT.as_secs()
            )),
        },
        Err(err) => CapturedOutput {
            text: String::new(),
            message: Some(format!("Failed to run command: {err}")),
        },
    }
}

/// Prompt for a line of input. Uses a rich interactive prompt when stdin is a real
/// terminal; falls back to plain line-reading (loop until non-empty, unless
/// `allow_empty`) when it isn't, so `new` stays usable when scripted or piped —
/// mirroring how Go pet's readline-based prompts degrade over a plain `io.Reader`.
fn scan(prompt: &str, allow_empty: bool) -> Result<String> {
    if io::stdin().is_terminal() {
        let value = Input::<String>::new()
            .with_prompt(prompt.trim_end())
            .allow_empty(allow_empty)
            .interact_text()?;
        Ok(value.trim().to_string())
    } else {
        scan_plain(prompt, allow_empty)
    }
}

fn scan_plain(prompt: &str, allow_empty: bool) -> Result<String> {
    let stdin = io::stdin();
    loop {
        print!("{prompt}");
        io::stdout().flush()?;

        let mut line = String::new();
        let bytes_read = stdin.lock().read_line(&mut line)?;
        if bytes_read == 0 {
            bail!("canceled");
        }

        let line = line.trim().to_string();
        if line.is_empty() && !allow_empty {
            continue;
        }
        return Ok(line);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultilineState {
    Start,
    LastLineNotEmpty,
    LastLineEmpty,
}

enum MultilineStep {
    Continue(MultilineState),
    Done,
}

/// Pure state transition for multiline command entry: two consecutive blank lines
/// (after at least one non-blank line) finish the snippet. Appends `line` to `buf`
/// as needed and returns the next state, kept separate from the stdin-reading loop
/// in `scan_multiline` so it's testable without any I/O.
fn multiline_step(state: MultilineState, buf: &mut String, line: &str) -> MultilineStep {
    match state {
        MultilineState::Start => {
            if line.is_empty() {
                MultilineStep::Continue(MultilineState::Start)
            } else {
                buf.push_str(line);
                MultilineStep::Continue(MultilineState::LastLineNotEmpty)
            }
        }
        MultilineState::LastLineNotEmpty => {
            if line.is_empty() {
                MultilineStep::Continue(MultilineState::LastLineEmpty)
            } else {
                buf.push('\n');
                buf.push_str(line);
                MultilineStep::Continue(MultilineState::LastLineNotEmpty)
            }
        }
        MultilineState::LastLineEmpty => {
            if line.is_empty() {
                MultilineStep::Done
            } else {
                buf.push('\n');
                buf.push_str(line);
                MultilineStep::Continue(MultilineState::LastLineNotEmpty)
            }
        }
    }
}

/// Reads lines from stdin until two consecutive blank lines are entered. Mirrors Go
/// pet's `scanMultiLine` state machine; EOF (Ctrl-D) before that cancels the whole snippet.
fn scan_multiline() -> Result<String> {
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut multiline = String::new();
    let mut state = MultilineState::Start;

    print!(
        "{} ",
        "Command>".if_supports_color(Stdout, |t| t.bright_yellow())
    );
    io::stdout().flush()?;

    loop {
        let mut line = String::new();
        let bytes_read = handle.read_line(&mut line)?;
        if bytes_read == 0 {
            bail!("canceled");
        }
        let line = line.trim_end_matches('\n');

        match multiline_step(state, &mut multiline, line) {
            MultilineStep::Done => return Ok(multiline),
            MultilineStep::Continue(next) => {
                if state == MultilineState::Start && next == MultilineState::LastLineNotEmpty {
                    print!(
                        "{} ",
                        "......>".if_supports_color(Stdout, |t| t.bright_yellow())
                    );
                    io::stdout().flush()?;
                }
                state = next;
            }
        }
    }
}

fn count_snippet_lines(general: &crate::config::GeneralConfig) -> Result<usize> {
    let path = expand_absolute(&general.snippetfile)?;
    let contents = std::fs::read_to_string(&path)?;
    Ok(contents.matches('\n').count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_lines(lines: &[&str]) -> Option<String> {
        let mut state = MultilineState::Start;
        let mut buf = String::new();
        for line in lines {
            match multiline_step(state, &mut buf, line) {
                MultilineStep::Done => return Some(buf),
                MultilineStep::Continue(next) => state = next,
            }
        }
        None
    }

    #[test]
    fn leading_blank_lines_are_ignored() {
        let mut state = MultilineState::Start;
        let mut buf = String::new();
        let MultilineStep::Continue(next) = multiline_step(state, &mut buf, "") else {
            panic!("expected Continue");
        };
        assert_eq!(next, MultilineState::Start);
        assert!(buf.is_empty());
        state = next;

        let MultilineStep::Continue(next) = multiline_step(state, &mut buf, "echo hi") else {
            panic!("expected Continue");
        };
        assert_eq!(next, MultilineState::LastLineNotEmpty);
        assert_eq!(buf, "echo hi");
    }

    #[test]
    fn single_blank_line_does_not_finish_the_snippet() {
        let result = run_lines(&["echo one", "echo two", ""]);
        assert_eq!(result, None);
    }

    #[test]
    fn two_consecutive_blank_lines_finish_the_snippet() {
        let result = run_lines(&["echo one", "echo two", "", ""]);
        assert_eq!(result, Some("echo one\necho two".to_string()));
    }

    #[test]
    fn a_blank_line_followed_by_more_input_keeps_going() {
        let result = run_lines(&["echo one", "", "echo two", "", ""]);
        assert_eq!(result, Some("echo one\necho two".to_string()));
    }

    #[test]
    fn single_line_command_needs_only_one_double_blank() {
        let result = run_lines(&["echo hi", "", ""]);
        assert_eq!(result, Some("echo hi".to_string()));
    }

    #[test]
    fn resolve_output_capture_disabled_never_calls_run_capture() {
        let result = resolve_output_capture(false, "echo hi", || {
            panic!("run_capture should not be called when capture_output is false")
        });
        assert_eq!(result.text, "");
        assert!(result.message.is_none());
    }

    #[test]
    fn resolve_output_capture_skips_commands_with_params() {
        let result = resolve_output_capture(true, "echo <name>", || {
            panic!("run_capture should not be called for a parameterized command")
        });
        assert_eq!(result.text, "");
        assert!(result.message.unwrap().contains("<param>"));
    }

    #[test]
    fn resolve_output_capture_stores_captured_text() {
        let result = resolve_output_capture(true, "echo hi", || {
            Ok(CaptureOutcome::Captured("hi\n".to_string()))
        });
        assert_eq!(result.text, "hi\n");
        assert!(result.message.is_some());
    }

    #[test]
    fn resolve_output_capture_failed_status_leaves_output_empty() {
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .status()
            .unwrap();
        let result = resolve_output_capture(true, "false", || Ok(CaptureOutcome::Failed(status)));
        assert_eq!(result.text, "");
        assert!(result.message.unwrap().contains("Command failed"));
    }

    #[test]
    fn resolve_output_capture_timed_out_leaves_output_empty() {
        let result = resolve_output_capture(true, "sleep 100", || Ok(CaptureOutcome::TimedOut));
        assert_eq!(result.text, "");
        assert!(result.message.unwrap().contains("timed out"));
    }

    #[test]
    fn resolve_output_capture_io_error_leaves_output_empty() {
        let result = resolve_output_capture(true, "nonexistent-binary", || {
            Err(io::Error::other("spawn failed"))
        });
        assert_eq!(result.text, "");
        assert!(result.message.unwrap().contains("Failed to run command"));
    }
}
