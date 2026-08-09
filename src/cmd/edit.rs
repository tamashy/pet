use anyhow::{Result, bail};

use crate::config::Config;
use crate::editor;
use crate::path::expand_absolute;
use crate::selector::{self, SelectOptions};
use crate::snippet::Snippets;

pub struct EditOptions {
    pub query: Option<String>,
    pub tag: Option<String>,
}

pub fn run(config: &Config, opts: EditOptions) -> Result<()> {
    let snippet_path = if config.general.snippetdirs.is_empty() {
        expand_absolute(&config.general.snippetfile)?
    } else {
        // Multiple snippet files could be in play, so the user picks which one —
        // matches Go pet's edit.go: selectFile() only runs when snippetdirs is set.
        let mut snippets = Snippets::load(&config.general, true)?;
        if let Some(tag) = &opts.tag {
            snippets.snippets = snippets.filter_by_single_tag(tag);
        }
        // No --color here: Go pet's selectFile (edit's file picker) never colors
        // its display text either.
        let select_opts = SelectOptions {
            query: opts.query.clone(),
            color: false,
        };
        match selector::select_file(&config.general, &snippets.snippets, &select_opts)? {
            Some(path) => path,
            None => bail!("no snippet file selected"),
        }
    };

    editor::open(&config.general, &snippet_path, 0)
}
