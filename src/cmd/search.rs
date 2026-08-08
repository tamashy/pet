use std::io::{self, IsTerminal, Write};

use anyhow::Result;

use crate::config::Config;
use crate::dialog;
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
    let mut commands =
        selector::select_commands(&config.general, &snippets.snippets, &select_opts)?;

    if commands.is_empty() {
        return Ok(());
    }

    // Parameter substitution only kicks in for a single selected snippet, matching
    // Go pet's cmd/util.go `filter()` exactly: a multi-select or --raw always
    // returns the stored command(s) verbatim, and a param-less command is a no-op
    // either way (extract_params returns empty).
    if !opts.raw && commands.len() == 1 {
        let params = dialog::extract_params(&commands[0]);
        if !params.is_empty() {
            match dialog::resolve_params(&params, &commands[0])? {
                Some(values) => commands[0] = dialog::substitute(&commands[0], &values),
                // Cancelled (Esc/Ctrl-C): print nothing, same as a cancelled
                // selector pick, rather than treating it as a hard error.
                None => return Ok(()),
            }
        }
    }

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
