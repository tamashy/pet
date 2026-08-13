use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::GeneralConfig;
use crate::error::SnippetError;
use crate::path::expand_absolute;

/// Invocation count and last-used time for one snippet, keyed by description in
/// `UsageStats::entries` (the same identity `cmd::new` already treats as unique).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UsageEntry {
    #[serde(default)]
    pub count: u64,
    /// Unix timestamp (seconds) of the most recent use.
    #[serde(default)]
    pub last_used: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageStats {
    #[serde(default)]
    pub entries: HashMap<String, UsageEntry>,
}

impl UsageStats {
    /// Load usage stats from `path`, or an empty `UsageStats` if the file doesn't
    /// exist yet (e.g. no snippet has ever been used).
    pub fn load(path: &Path) -> Result<UsageStats, SnippetError> {
        if !path.exists() {
            return Ok(UsageStats::default());
        }

        let contents = std::fs::read_to_string(path).map_err(|source| SnippetError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&contents).map_err(|source| SnippetError::Parse {
            path: path.to_path_buf(),
            source: Box::new(source),
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), SnippetError> {
        let serialized = toml::to_string_pretty(self)?;
        std::fs::write(path, serialized).map_err(|source| SnippetError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Record one use of the snippet identified by `description` at the current time.
    pub fn record(&mut self, description: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let entry = self.entries.entry(description.to_string()).or_default();
        entry.count += 1;
        entry.last_used = now;
    }

    /// Sort key for a snippet: unused snippets sort as `(0, 0)`, i.e. last.
    pub fn score(&self, description: &str) -> UsageEntry {
        self.entries.get(description).copied().unwrap_or_default()
    }
}

/// Where usage stats live: a `usage.toml` next to `general.snippetfile`. Kept
/// separate from `snippet.toml` itself so stats stay local-only and don't ride
/// along with a shared/synced snippet file.
pub fn file_path(general: &GeneralConfig) -> Result<PathBuf, SnippetError> {
    let snippet_file_path =
        expand_absolute(&general.snippetfile).map_err(|source| SnippetError::Read {
            path: PathBuf::from(&general.snippetfile),
            source,
        })?;
    let dir = snippet_file_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(dir.join("usage.toml"))
}

/// Load, record one use for each of `descriptions`, and save back — the whole
/// read-modify-write cycle `search`/`exec`/`clip` need after a successful selection.
pub fn record_uses<'a, I>(general: &GeneralConfig, descriptions: I) -> Result<(), SnippetError>
where
    I: IntoIterator<Item = &'a str>,
{
    let path = file_path(general)?;
    let mut stats = UsageStats::load(&path)?;
    for description in descriptions {
        stats.record(description);
    }
    stats.save(&path)
}

/// Drop stats for descriptions that no longer have a matching snippet (called by
/// `cmd::delete` so `usage.toml` doesn't accumulate stale entries forever).
pub fn remove_uses<'a, I>(general: &GeneralConfig, descriptions: I) -> Result<(), SnippetError>
where
    I: IntoIterator<Item = &'a str>,
{
    let path = file_path(general)?;
    let mut stats = UsageStats::load(&path)?;
    for description in descriptions {
        stats.entries.remove(description);
    }
    stats.save(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_increments_count_and_updates_last_used() {
        let mut stats = UsageStats::default();
        stats.record("greet");
        assert_eq!(stats.score("greet").count, 1);
        assert!(stats.score("greet").last_used > 0);

        stats.record("greet");
        assert_eq!(stats.score("greet").count, 2);
    }

    #[test]
    fn score_of_unused_snippet_is_zero() {
        let stats = UsageStats::default();
        assert_eq!(stats.score("never-used"), UsageEntry::default());
    }

    #[test]
    fn load_of_missing_file_returns_default() {
        let dir = std::env::temp_dir().join("pet-usage-test-missing");
        let path = dir.join("usage.toml");
        let stats = UsageStats::load(&path).unwrap();
        assert!(stats.entries.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.toml");

        let mut stats = UsageStats::default();
        stats.record("greet");
        stats.save(&path).unwrap();

        let reloaded = UsageStats::load(&path).unwrap();
        assert_eq!(reloaded.score("greet").count, 1);
    }

    #[test]
    fn file_path_sits_next_to_the_snippet_file() {
        let general = GeneralConfig {
            snippetfile: "/home/user/.config/pet/snippet.toml".to_string(),
            ..GeneralConfig::default()
        };
        let path = file_path(&general).unwrap();
        assert_eq!(path, PathBuf::from("/home/user/.config/pet/usage.toml"));
    }
}
