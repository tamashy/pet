use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::GeneralConfig;
use crate::error::SnippetError;
use crate::path::{expand_absolute, files_in_dir};
use crate::usage::{self, UsageStats};

// Aliases accept the PascalCase keys pelletier/go-toml emits for untagged Go struct
// fields (Description/Tag/Output have no `toml:"..."` tag in Go pet), so snippet
// files from older pet installs still parse correctly. See config.rs for the same
// pattern and rationale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnippetInfo {
    #[serde(skip)]
    pub filename: PathBuf,
    #[serde(default, alias = "Description")]
    pub description: String,
    #[serde(alias = "Command")]
    pub command: String,
    #[serde(default, rename = "tag", alias = "Tag")]
    pub tag: Vec<String>,
    #[serde(default, alias = "Output")]
    pub output: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snippets {
    #[serde(default)]
    pub snippets: Vec<SnippetInfo>,
}

impl Snippets {
    /// Load snippets from the main snippet file, and (if `include_dirs`) every
    /// file found directly inside each configured snippet directory. Mirrors
    /// Go pet's `Snippets.Load`.
    pub fn load(general: &GeneralConfig, include_dirs: bool) -> Result<Snippets, SnippetError> {
        let mut snippet_files: Vec<PathBuf> = Vec::new();

        let snippet_file_path =
            expand_absolute(&general.snippetfile).map_err(|source| SnippetError::Read {
                path: PathBuf::from(&general.snippetfile),
                source,
            })?;

        if snippet_file_path.exists() {
            snippet_files.push(snippet_file_path);
        } else {
            return Err(SnippetError::SnippetFileNotFound(snippet_file_path));
        }

        if include_dirs {
            for dir in &general.snippetdirs {
                let abs_dir = expand_absolute(dir).map_err(|source| SnippetError::Read {
                    path: PathBuf::from(dir),
                    source,
                })?;
                if !abs_dir.exists() {
                    return Err(SnippetError::SnippetDirNotFound(abs_dir));
                }
                let files = files_in_dir(&abs_dir).map_err(|source| SnippetError::Read {
                    path: abs_dir.clone(),
                    source,
                })?;
                snippet_files.extend(files);
            }
        }

        let mut snippets = Snippets::default();
        for file in snippet_files {
            let contents = std::fs::read_to_string(&file).map_err(|source| SnippetError::Read {
                path: file.clone(),
                source,
            })?;
            let mut parsed: Snippets =
                toml::from_str(&contents).map_err(|source| SnippetError::Parse {
                    path: file.clone(),
                    source: Box::new(source),
                })?;
            for snippet in &mut parsed.snippets {
                snippet.filename = file.clone();
            }
            snippets.snippets.extend(parsed.snippets);
        }

        let usage_stats = UsageStats::load(&usage::file_path(general)?)?;
        snippets.order(&general.sortby, &usage_stats);
        Ok(snippets)
    }

    /// Save snippets back to their originating files, rewriting each file in full.
    /// Snippets with no `filename` (freshly created) go to `general.snippetfile`.
    pub fn save(&self, general: &GeneralConfig) -> Result<(), SnippetError> {
        let mut by_file: HashMap<PathBuf, Vec<SnippetInfo>> = HashMap::new();

        for snippet in &self.snippets {
            let filename = if snippet.filename.as_os_str().is_empty() {
                PathBuf::from(&general.snippetfile)
            } else {
                snippet.filename.clone()
            };
            by_file.entry(filename).or_default().push(snippet.clone());
        }

        for (file, snippets) in by_file {
            let abs_path =
                expand_absolute(&file.to_string_lossy()).map_err(|source| SnippetError::Write {
                    path: file.clone(),
                    source,
                })?;
            let serialized = toml::to_string_pretty(&Snippets { snippets })?;
            std::fs::write(&abs_path, serialized).map_err(|source| SnippetError::Write {
                path: abs_path,
                source,
            })?;
        }

        Ok(())
    }

    /// Sort snippets in place according to the `sortby` config value.
    /// Supported: recency (default, no-op), -recency, [+-]description, [+-]command,
    /// [+-]output, [+-]usage (most-invoked-and-most-recently-used first; -usage
    /// reverses it). `usage` needs invocation stats recorded by `usage::record_uses`,
    /// passed in via `usage_stats` (an unused snippet sorts as if never used).
    pub fn order(&mut self, sortby: &str, usage_stats: &UsageStats) {
        match sortby {
            "command" | "+command" => self.snippets.sort_by(|a, b| b.command.cmp(&a.command)),
            "-command" => self.snippets.sort_by(|a, b| a.command.cmp(&b.command)),
            "description" | "+description" => self
                .snippets
                .sort_by(|a, b| b.description.cmp(&a.description)),
            "-description" => self
                .snippets
                .sort_by(|a, b| a.description.cmp(&b.description)),
            "output" | "+output" => self.snippets.sort_by(|a, b| b.output.cmp(&a.output)),
            "-output" => self.snippets.sort_by(|a, b| a.output.cmp(&b.output)),
            "usage" | "+usage" => self.snippets.sort_by(|a, b| {
                usage_stats
                    .score(&b.description)
                    .cmp(&usage_stats.score(&a.description))
            }),
            "-usage" => self.snippets.sort_by(|a, b| {
                usage_stats
                    .score(&a.description)
                    .cmp(&usage_stats.score(&b.description))
            }),
            "-recency" => self.snippets.reverse(),
            _ => {}
        }
    }

    /// Keep only snippets that have at least one tag in common with `tags`.
    /// Mirrors Go pet's `Snippets.FilterByTags` (used by `list -t`).
    pub fn filter_by_tags(&self, tags: &[String]) -> Vec<SnippetInfo> {
        self.snippets
            .iter()
            .filter(|s| !s.tag.is_empty() && s.tag.iter().any(|t| tags.contains(t)))
            .cloned()
            .collect()
    }

    /// Keep only snippets whose tags contain exactly `tag`. Mirrors the single-tag
    /// filter inlined in Go pet's `cmd/util.go` `filter()`, used by
    /// `search`/`exec`/`clip`/`edit` — distinct from `filter_by_tags`, which is
    /// `list`'s comma-separated multi-tag filter.
    pub fn filter_by_single_tag(&self, tag: &str) -> Vec<SnippetInfo> {
        self.snippets
            .iter()
            .filter(|s| s.tag.iter().any(|t| t == tag))
            .cloned()
            .collect()
    }

    /// Keep only snippets whose description or command contains `needle`
    /// (case-insensitive). Used by `list -f`/`search -f` to narrow a long
    /// snippet list by content instead of only by tag.
    pub fn filter_by_text(&self, needle: &str) -> Vec<SnippetInfo> {
        let needle = needle.to_lowercase();
        self.snippets
            .iter()
            .filter(|s| {
                s.description.to_lowercase().contains(&needle)
                    || s.command.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(description: &str) -> SnippetInfo {
        SnippetInfo {
            filename: PathBuf::new(),
            description: description.to_string(),
            command: String::new(),
            tag: vec![],
            output: String::new(),
        }
    }

    #[test]
    fn order_on_empty_snippets_does_not_panic() {
        let mut snippets = Snippets::default();
        snippets.order("description", &UsageStats::default());
        assert!(snippets.snippets.is_empty());
    }

    #[test]
    fn order_unknown_sortby_leaves_original_order_unchanged() {
        let mut snippets = Snippets {
            snippets: vec![snippet("b"), snippet("a"), snippet("c")],
        };
        snippets.order("not-a-real-sortby", &UsageStats::default());
        let descs: Vec<_> = snippets.snippets.iter().map(|s| &s.description).collect();
        assert_eq!(descs, vec!["b", "a", "c"]);
    }

    #[test]
    fn order_empty_sortby_is_recency_noop() {
        let mut snippets = Snippets {
            snippets: vec![snippet("first"), snippet("second")],
        };
        snippets.order("", &UsageStats::default());
        let descs: Vec<_> = snippets.snippets.iter().map(|s| &s.description).collect();
        assert_eq!(descs, vec!["first", "second"]);
    }

    #[test]
    fn order_usage_sorts_most_used_first() {
        // last_used has only second resolution, so don't assert a tie-break order
        // between snippets used within the same second — just count-based ranking.
        let mut usage_stats = UsageStats::default();
        usage_stats.record("a"); // count 1
        usage_stats.record("c");
        usage_stats.record("c"); // count 2, most used overall

        let mut snippets = Snippets {
            snippets: vec![snippet("a"), snippet("c"), snippet("unused")],
        };
        snippets.order("usage", &usage_stats);
        let descs: Vec<_> = snippets.snippets.iter().map(|s| &s.description).collect();
        assert_eq!(descs, vec!["c", "a", "unused"]);

        snippets.order("-usage", &usage_stats);
        let descs: Vec<_> = snippets.snippets.iter().map(|s| &s.description).collect();
        assert_eq!(descs, vec!["unused", "a", "c"]);
    }

    #[test]
    fn filter_by_tags_on_empty_snippets_returns_empty() {
        let snippets = Snippets::default();
        assert!(snippets.filter_by_tags(&["x".to_string()]).is_empty());
    }

    #[test]
    fn filter_by_single_tag_requires_exact_match() {
        let mut a = snippet("a");
        a.tag = vec!["net".to_string()];
        let mut b = snippet("b");
        b.tag = vec!["network".to_string()];
        let c = snippet("c");
        let snippets = Snippets {
            snippets: vec![a, b, c],
        };

        let result = snippets.filter_by_single_tag("net");
        let descs: Vec<_> = result.iter().map(|s| s.description.as_str()).collect();
        assert_eq!(descs, vec!["a"]);
    }

    #[test]
    fn filter_by_text_matches_description_or_command_case_insensitively() {
        let mut by_description = snippet("Compress a Docker context");
        by_description.command = "tar -czf out.tar.gz .".to_string();
        let mut by_command = snippet("archive a directory");
        by_command.command = "docker save -o out.tar my-image".to_string();
        let no_match = snippet("ping a host");
        let snippets = Snippets {
            snippets: vec![by_description, by_command, no_match],
        };

        let result = snippets.filter_by_text("DOCKER");
        let descs: Vec<_> = result.iter().map(|s| s.description.as_str()).collect();
        assert_eq!(
            descs,
            vec!["Compress a Docker context", "archive a directory"]
        );
    }

    #[test]
    fn filter_by_text_on_empty_snippets_returns_empty() {
        let snippets = Snippets::default();
        assert!(snippets.filter_by_text("anything").is_empty());
    }
}
