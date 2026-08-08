use std::io::{self, IsTerminal, Write};

use anyhow::Result;

use crate::config::Config;
use crate::selector::{self, SelectOptions};
use crate::snippet::Snippets;

pub struct SearchOptions {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub delimiter: String,
    pub raw: bool,
}

pub fn run(config: &Config, opts: SearchOptions) -> Result<()> {
    let mut snippets = Snippets::load(&config.general, true)?;
    if let Some(tag) = &opts.tag {
        snippets.snippets = snippets.filter_by_single_tag(tag);
    }

    let select_opts = SelectOptions {
        query: opts.query.clone(),
    };
    let commands = selector::select_commands(&config.general, &snippets.snippets, &select_opts)?;

    if commands.is_empty() {
        return Ok(());
    }

    // Parameter substitution (<name>/<name=default>) lands with dialog.rs; until
    // then every selection prints its stored command verbatim, i.e. every search
    // currently behaves like --raw regardless of the flag's value.
    let _ = opts.raw;

    let mut output = commands.join(&opts.delimiter);
    if io::stdout().is_terminal() {
        output.push('\n');
    }
    write_ignoring_broken_pipe(&output)
}

/// A downstream reader closing early (`pet search | head -1`, or a shell widget
/// that only consumes part of the output) is normal, not an error — print! would
/// panic on it since Rust ignores SIGPIPE by default and turns the write failure
/// into a plain io::Error instead of terminating the process.
fn write_ignoring_broken_pipe(s: &str) -> Result<()> {
    match io::stdout().write_all(s.as_bytes()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}
