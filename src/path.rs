use std::path::{Path, PathBuf};

/// Expand a leading `~` (home dir) and resolve the result to an absolute path,
/// without requiring the path to exist. Mirrors Go pet's `path.NewAbsolutePath`.
pub fn expand_absolute(raw: &str) -> std::io::Result<PathBuf> {
    let expanded: PathBuf = if let Some(rest) = raw.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest)
    } else if raw == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(raw)
    };

    std::path::absolute(&expanded)
}

/// List the immediate files in `dir` (non-recursive), matching Go pet's snippetdirs scan.
pub fn files_in_dir(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}
