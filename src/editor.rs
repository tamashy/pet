use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::GeneralConfig;
use crate::shell;

/// Open `path` in the configured editor at `line` (0 for "no particular line").
/// Mirrors Go pet's `editFile`, which appends `+<line> <path>` to the editor command.
pub fn open(general: &GeneralConfig, path: &Path, line: usize) -> Result<()> {
    let command = format!("{} +{} {}", general.editor, line, path.display());
    let status = shell::spawn_inherit(general, &command)
        .with_context(|| format!("failed to launch editor: {}", general.editor))?;

    if !status.success() {
        bail!("editor exited with {status}");
    }
    Ok(())
}
