use anyhow::{Result, bail};

use crate::config::Config;
use crate::dialog;
use crate::selector::{self, SelectOptions};
use crate::shell;
use crate::snippet::Snippets;

pub struct ExecOptions {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub silent: bool,
}

pub fn run(config: &Config, opts: ExecOptions) -> Result<()> {
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

    // exec always attempts substitution (Go pet hardcodes raw=false here), still
    // gated to a single selection with actual params — see cmd::search.
    if commands.len() == 1 {
        let params = dialog::extract_params(&commands[0]);
        if !params.is_empty() {
            match dialog::resolve_params(&params, &commands[0])? {
                Some(values) => commands[0] = dialog::substitute(&commands[0], &values),
                None => return Ok(()),
            }
        }
    }

    let command = commands.join("; ");
    if !opts.silent {
        println!("> {command}");
    }

    let status = shell::spawn_inherit(&config.general, &command)?;
    if !status.success() {
        bail!("command exited with {status}");
    }
    Ok(())
}
