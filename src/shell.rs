use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::GeneralConfig;

/// Spawn `command_str` through the configured shell (`general.cmd`, default `["sh", "-c"]`),
/// inheriting stdin/stdout/stderr. Used for the editor and (later) `exec`. Mirrors Go pet's
/// `run()` helper when invoked interactively.
pub fn spawn_inherit(general: &GeneralConfig, command_str: &str) -> io::Result<ExitStatus> {
    let mut cmd = build_command(general, command_str);
    cmd.status()
}

fn build_command(general: &GeneralConfig, command_str: &str) -> Command {
    let mut cmd = if general.cmd.is_empty() {
        let mut c = Command::new("sh");
        c.arg("-c");
        c
    } else {
        let mut c = Command::new(&general.cmd[0]);
        c.args(&general.cmd[1..]);
        c
    };
    cmd.arg(command_str);
    cmd
}

/// Cap on captured stdout, so `pet new -o` can't balloon `snippet.toml` on a
/// command that produces a lot of output. Applied at read time (not by
/// truncating after the fact) so we never buffer more than this much in memory.
const MAX_CAPTURE_BYTES: usize = 4096;

pub enum CaptureOutcome {
    Captured(String),
    Failed(ExitStatus),
    TimedOut,
}

/// Run `command_str` through the configured shell and capture its stdout (only
/// — stderr is discarded, matching `$(...)` shell substitution semantics `pet`
/// already relies on elsewhere), for `pet new -o`. Stdin is closed so a command
/// that reads from it can't hang waiting for input that will never come.
///
/// Reads stdout on a dedicated thread while the main thread polls
/// `Child::try_wait()` against `timeout`, rather than waiting for exit and then
/// reading: without a concurrent reader, a command that writes more than the OS
/// pipe buffer would block forever on a full pipe while `try_wait()` never
/// returns `Some` (it's not exited, just stuck writing) — this is the standard
/// way to avoid that deadlock. On timeout, the child is killed.
pub fn capture_stdout(
    general: &GeneralConfig,
    command_str: &str,
    timeout: Duration,
) -> io::Result<CaptureOutcome> {
    let mut cmd = build_command(general, command_str);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::null());

    let mut child = cmd.spawn()?;
    let mut stdout = child.stdout.take().expect("stdout was piped");

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout)
            .take(MAX_CAPTURE_BYTES as u64 + 1)
            .read_to_end(&mut buf);
        let _ = tx.send(buf);
    });

    let status = wait_with_timeout(&mut child, timeout)?;
    let Some(status) = status else {
        return Ok(CaptureOutcome::TimedOut);
    };

    // The reader thread should already be done (or nearly done) by the time
    // the child has exited — a short bound here is just a safety net, not the
    // primary wait.
    let buf = rx.recv_timeout(Duration::from_secs(1)).unwrap_or_default();

    if !status.success() {
        return Ok(CaptureOutcome::Failed(status));
    }

    let truncated = buf.len() > MAX_CAPTURE_BYTES;
    // Trim trailing whitespace (almost always just the newline `echo`-style
    // commands end with) so it doesn't show up as a dangling blank line
    // wherever `output` gets rendered.
    let mut text = String::from_utf8_lossy(&buf[..buf.len().min(MAX_CAPTURE_BYTES)])
        .trim_end()
        .to_string();
    if truncated {
        text.push_str("\n... (output truncated)");
    }
    Ok(CaptureOutcome::Captured(text))
}

/// Poll `child` for exit, sleeping briefly between checks, until `timeout`
/// elapses — `None` means it timed out (the child is killed before returning).
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GeneralConfig;

    #[test]
    fn capture_stdout_returns_captured_text_on_success() {
        let outcome = capture_stdout(
            &GeneralConfig::default(),
            "printf hi",
            Duration::from_secs(5),
        )
        .unwrap();
        match outcome {
            CaptureOutcome::Captured(text) => assert_eq!(text, "hi"),
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn capture_stdout_trims_trailing_newline() {
        // `echo` (unlike `printf`) always ends its output with a newline —
        // confirm that doesn't survive into the stored text as a dangling
        // blank line wherever `output` gets rendered.
        let outcome =
            capture_stdout(&GeneralConfig::default(), "echo hi", Duration::from_secs(5)).unwrap();
        match outcome {
            CaptureOutcome::Captured(text) => assert_eq!(text, "hi"),
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn capture_stdout_reports_nonzero_exit_as_failed() {
        let outcome =
            capture_stdout(&GeneralConfig::default(), "exit 1", Duration::from_secs(5)).unwrap();
        assert!(matches!(outcome, CaptureOutcome::Failed(_)));
    }

    #[test]
    fn capture_stdout_times_out_on_a_command_that_never_finishes() {
        let outcome = capture_stdout(
            &GeneralConfig::default(),
            "sleep 5",
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(matches!(outcome, CaptureOutcome::TimedOut));
    }

    #[test]
    fn capture_stdout_truncates_output_past_the_cap_without_hanging() {
        // More than MAX_CAPTURE_BYTES, and — without the concurrent reader
        // thread — enough to fill the OS pipe buffer and deadlock a naive
        // wait-then-read implementation. A generous timeout here (5s) means
        // this test only passes quickly if the deadlock-avoidance actually works.
        let outcome = capture_stdout(
            &GeneralConfig::default(),
            "yes x | head -c 8000",
            Duration::from_secs(5),
        )
        .unwrap();
        match outcome {
            CaptureOutcome::Captured(text) => {
                assert!(text.ends_with("... (output truncated)"));
                assert!(text.len() < 8000);
            }
            _ => panic!("expected Captured"),
        }
    }
}
