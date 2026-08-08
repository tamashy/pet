use std::io::IsTerminal;

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

    print!("{}", commands.join(&opts.delimiter));
    if std::io::stdout().is_terminal() {
        println!();
    }

    Ok(())
}
