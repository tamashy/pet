use anyhow::{Context, Result};
use owo_colors::{OwoColorize, Stream::Stdout};

use crate::config::Config;
use crate::dialog;
use crate::selector::{self, SelectOptions};
use crate::snippet::Snippets;

pub struct ClipOptions {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub delimiter: String,
    pub raw: bool,
    pub show_command: bool,
    pub color: bool,
}

pub fn run(config: &Config, opts: ClipOptions) -> Result<()> {
    let mut snippets = Snippets::load(&config.general, true)?;
    if let Some(tag) = &opts.tag {
        snippets.snippets = snippets.filter_by_single_tag(tag);
    }

    let select_opts = SelectOptions {
        query: opts.query.clone(),
        color: opts.color,
    };
    let mut commands =
        selector::select_commands(&config.general, &snippets.snippets, &select_opts)?;

    if commands.is_empty() {
        return Ok(());
    }

    if !opts.raw && commands.len() == 1 {
        let params = dialog::extract_params(&commands[0]);
        if !params.is_empty() {
            match dialog::resolve_params(&params, &commands[0])? {
                Some(values) => commands[0] = dialog::substitute(&commands[0], &values),
                None => return Ok(()),
            }
        }
    }

    let command = commands.join(&opts.delimiter);

    if opts.show_command && !command.is_empty() {
        println!(
            "{} {command}",
            "Command:".if_supports_color(Stdout, |t| t.bright_yellow())
        );
    }

    let mut clipboard = arboard::Clipboard::new().context("failed to access clipboard")?;
    clipboard
        .set_text(command)
        .context("failed to copy command to clipboard")?;
    Ok(())
}
