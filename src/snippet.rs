use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::GeneralConfig;
use crate::error::SnippetError;
use crate::path::{expand_absolute, files_in_dir};

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

        snippets.order(&general.sortby);
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
    /// Supported: recency (default, no-op), -recency, [+-]description, [+-]command, [+-]output.
    pub fn order(&mut self, sortby: &str) {
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
}
