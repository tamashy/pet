use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use owo_colors::{OwoColorize, Stream::Stdout};

use crate::config::Config;
use crate::selector::{self, SelectOptions};
use crate::snippet::Snippets;
use crate::usage;

pub struct DeleteOptions {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub color: bool,
}

pub fn run(config: &Config, opts: DeleteOptions) -> Result<()> {
    let mut snippets = Snippets::load(&config.general, true)?;
    if let Some(tag) = &opts.tag {
        snippets.snippets = snippets.filter_by_single_tag(tag);
    }

    let select_opts = SelectOptions {
        query: opts.query.clone(),
        color: opts.color,
    };
    let selected = selector::select_snippets(&config.general, &snippets.snippets, &select_opts)?;
    if selected.is_empty() {
        return Ok(());
    }

    let mut all = Snippets::load(&config.general, true)?;
    let touched_files: HashSet<PathBuf> = selected.iter().map(|s| s.filename.clone()).collect();
    all.snippets.retain(|s| !selected.contains(s));
    all.save(&config.general)?;
    usage::remove_uses(
        &config.general,
        selected.iter().map(|s| s.description.as_str()),
    )?;

    // Snippets::save only rewrites files that still have at least one snippet in
    // them — if a delete emptied a file out entirely, it never appears in that
    // pass and the stale entries would otherwise linger on disk.
    let remaining_files: HashSet<PathBuf> =
        all.snippets.iter().map(|s| s.filename.clone()).collect();
    for file in touched_files {
        if !remaining_files.contains(&file) {
            std::fs::write(&file, "")
                .with_context(|| format!("failed to write {}", file.display()))?;
        }
    }

    for s in &selected {
        println!(
            "{} {}",
            "Deleted:".if_supports_color(Stdout, |t| t.bright_red()),
            s.description
        );
    }

    Ok(())
}
