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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_slash_to_home_dir() {
        let home = dirs::home_dir().unwrap();
        let result = expand_absolute("~/snippets/foo.toml").unwrap();
        assert_eq!(result, home.join("snippets/foo.toml"));
    }

    #[test]
    fn expands_bare_tilde_to_home_dir() {
        let home = dirs::home_dir().unwrap();
        let result = expand_absolute("~").unwrap();
        assert_eq!(result, home);
    }

    #[test]
    fn leaves_absolute_paths_unchanged() {
        let result = expand_absolute("/etc/hosts").unwrap();
        assert_eq!(result, PathBuf::from("/etc/hosts"));
    }

    #[test]
    fn resolves_relative_paths_against_cwd() {
        let result = expand_absolute("some/relative/path").unwrap();
        assert!(result.is_absolute());
        assert!(result.ends_with("some/relative/path"));
    }

    #[test]
    fn does_not_require_the_path_to_exist() {
        assert!(expand_absolute("/definitely/does/not/exist/anywhere").is_ok());
    }
}
