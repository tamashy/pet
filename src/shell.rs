use std::io;
use std::process::{Command, ExitStatus};

use crate::config::GeneralConfig;

/// Spawn `command_str` through the configured shell (`general.cmd`, default `["sh", "-c"]`),
/// inheriting stdin/stdout/stderr. Used for the editor and (later) `exec`. Mirrors Go pet's
/// `run()` helper when invoked interactively.
pub fn spawn_inherit(general: &GeneralConfig, command_str: &str) -> io::Result<ExitStatus> {
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
    cmd.status()
}
