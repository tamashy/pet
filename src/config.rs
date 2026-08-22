use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

fn default_column() -> i32 {
    40
}

fn default_selectcmd() -> String {
    "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
        .to_string()
}

fn default_cmd() -> Vec<String> {
    vec!["sh".to_string(), "-c".to_string()]
}

fn default_format() -> String {
    "[$description]: $command $tags".to_string()
}

fn default_backend() -> String {
    "gist".to_string()
}

// Aliases below accept the PascalCase keys that pelletier/go-toml emits for any Go
// struct field without an explicit `toml:"..."` tag. Go pet's `GeneralConfig` has no
// tags at all, so real-world config.toml files (including ones from older pet
// installs) commonly use `Editor`, `SnippetFile`, `SortBy`, etc. instead of the
// lowercase names shown in the README. Accept both so existing files load correctly;
// we still always *write* lowercase (matching the currently-documented format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default, alias = "SnippetFile")]
    pub snippetfile: String,
    #[serde(default, alias = "SnippetDirs")]
    pub snippetdirs: Vec<String>,
    #[serde(default, alias = "Editor")]
    pub editor: String,
    #[serde(default = "default_column", alias = "Column")]
    pub column: i32,
    #[serde(default = "default_selectcmd", alias = "SelectCmd")]
    pub selectcmd: String,
    #[serde(default = "default_backend", alias = "Backend")]
    pub backend: String,
    #[serde(default, alias = "SortBy")]
    pub sortby: String,
    #[serde(default = "default_cmd", alias = "Cmd")]
    pub cmd: Vec<String>,
    #[serde(default, alias = "Color")]
    pub color: bool,
    #[serde(default = "default_format", alias = "Format")]
    pub format: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            snippetfile: String::new(),
            snippetdirs: Vec::new(),
            editor: String::new(),
            column: default_column(),
            // Same reasoning as `color` below: only the value baked into freshly-
            // generated config.toml files. The field's own `#[serde(default)]`
            // stays the fzf invocation, so a config.toml missing the key entirely
            // still behaves exactly like it always did. "builtin" is `selector`'s
            // sentinel for the native picker in picker.rs — no external tool
            // required for a brand-new install.
            selectcmd: crate::selector::BUILTIN_SELECTCMD.to_string(),
            backend: default_backend(),
            sortby: String::new(),
            cmd: default_cmd(),
            // Only the value baked into freshly-generated config.toml files
            // (Config::load's first-run path) — the `color` field's own
            // `#[serde(default)]` stays bool::default() (false) so a config.toml
            // that omits the key entirely (e.g. an old Go pet config someone
            // points --config at) keeps behaving exactly like it always did.
            color: true,
            format: default_format(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GistConfig {
    #[serde(default, rename = "file_name")]
    pub file_name: String,
    #[serde(default, rename = "access_token")]
    pub access_token: String,
    #[serde(default, rename = "gist_id")]
    pub gist_id: String,
    #[serde(default, alias = "Public")]
    pub public: bool,
    #[serde(default, rename = "auto_sync")]
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GitLabConfig {
    #[serde(default, rename = "file_name")]
    pub file_name: String,
    #[serde(default, rename = "access_token")]
    pub access_token: String,
    #[serde(default, alias = "Url")]
    pub url: String,
    #[serde(default, alias = "ID")]
    pub id: String,
    #[serde(default, alias = "Visibility")]
    pub visibility: String,
    #[serde(default, rename = "auto_sync")]
    pub auto_sync: bool,
    #[serde(default, rename = "skip_ssl")]
    pub skip_ssl: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GheGistConfig {
    #[serde(default, rename = "base_url")]
    pub base_url: String,
    #[serde(default, rename = "upload_url")]
    pub upload_url: String,
    #[serde(default, rename = "file_name")]
    pub file_name: String,
    #[serde(default, rename = "access_token")]
    pub access_token: String,
    #[serde(default, rename = "gist_id")]
    pub gist_id: String,
    #[serde(default, alias = "Public")]
    pub public: bool,
    #[serde(default, rename = "auto_sync")]
    pub auto_sync: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default, rename = "General")]
    pub general: GeneralConfig,
    #[serde(default, rename = "Gist")]
    pub gist: GistConfig,
    #[serde(default, rename = "GitLab")]
    pub gitlab: GitLabConfig,
    #[serde(default, rename = "GHEGist")]
    pub ghe_gist: GheGistConfig,
}

impl Config {
    /// Load config from `path`, creating a default config (and an empty snippet
    /// file next to it) if none exists yet. Mirrors Go pet's `Config.Load`.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        if path.exists() {
            let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
            let cfg: Config = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source: Box::new(source),
            })?;
            return Ok(cfg);
        }

        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir).map_err(|source| ConfigError::Write {
            path: dir.to_path_buf(),
            source,
        })?;

        let snippet_file = dir.join("snippet.toml");
        std::fs::write(&snippet_file, "").map_err(|source| ConfigError::Write {
            path: snippet_file.clone(),
            source,
        })?;

        let cfg = Config {
            general: GeneralConfig {
                snippetfile: snippet_file.to_string_lossy().into_owned(),
                editor: default_editor(),
                ..GeneralConfig::default()
            },
            gist: GistConfig {
                file_name: "pet-snippet.toml".to_string(),
                ..GistConfig::default()
            },
            gitlab: GitLabConfig {
                file_name: "pet-snippet.toml".to_string(),
                visibility: "private".to_string(),
                ..GitLabConfig::default()
            },
            ghe_gist: GheGistConfig::default(),
        };

        cfg.save(path)?;

        Ok(cfg)
    }

    /// Serialize and write `self` back to `path`. Used by `pet sync push` to
    /// persist a newly-created gist_id back into config.toml.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let serialized = toml::to_string_pretty(self)?;
        std::fs::write(path, serialized).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

fn default_editor() -> String {
    let env_editor = std::env::var("EDITOR")
        .ok()
        .filter(|editor| !editor.is_empty());
    pick_editor(env_editor, cfg!(windows), || {
        is_command_available("sensible-editor")
    })
}

/// Pure decision logic behind `default_editor`, split out so it's testable without
/// touching real env vars or spawning a subprocess. `sensible_editor_available` is
/// lazy (only called when actually needed) to match the original short-circuiting.
fn pick_editor(
    env_editor: Option<String>,
    is_windows: bool,
    sensible_editor_available: impl FnOnce() -> bool,
) -> String {
    if let Some(editor) = env_editor {
        return editor;
    }
    if is_windows {
        return String::new();
    }
    if sensible_editor_available() {
        "sensible-editor".to_string()
    } else {
        "vim".to_string()
    }
}

fn is_command_available(name: &str) -> bool {
    // Go's exec.Cmd discards Stdout/Stderr by default when unset; std::process::Command
    // inherits them instead, so without this the `command -v` output (e.g. a resolved
    // path) leaks straight into pet's own stdout on first run.
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Resolve the default config directory, honoring `PET_CONFIG_DIR`, and ensure it exists.
pub fn default_config_dir() -> Result<PathBuf, ConfigError> {
    let dir = if let Ok(env_dir) = std::env::var("PET_CONFIG_DIR") {
        PathBuf::from(env_dir)
    } else if cfg!(windows) {
        let appdata = std::env::var("APPDATA").ok().map(PathBuf::from);
        let base = appdata.unwrap_or_else(|| {
            let profile = std::env::var("USERPROFILE").unwrap_or_default();
            PathBuf::from(profile).join("Application Data")
        });
        base.join("pet")
    } else {
        let home = dirs::home_dir().ok_or(ConfigError::NoConfigDir)?;
        home.join(".config").join("pet")
    };

    std::fs::create_dir_all(&dir).map_err(|source| ConfigError::Write {
        path: dir.clone(),
        source,
    })?;

    std::path::absolute(&dir).map_err(|source| ConfigError::Write {
        path: dir.clone(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_editor_prefers_env_var_over_everything_else() {
        let editor = pick_editor(Some("nvim".to_string()), false, || {
            panic!("sensible-editor check should not run when $EDITOR is set")
        });
        assert_eq!(editor, "nvim");
    }

    #[test]
    fn pick_editor_on_windows_without_env_var_is_empty() {
        let editor = pick_editor(None, true, || {
            panic!("sensible-editor check should not run on windows")
        });
        assert_eq!(editor, "");
    }

    #[test]
    fn pick_editor_falls_back_to_sensible_editor_when_available() {
        let editor = pick_editor(None, false, || true);
        assert_eq!(editor, "sensible-editor");
    }

    #[test]
    fn pick_editor_falls_back_to_vim_when_sensible_editor_missing() {
        let editor = pick_editor(None, false, || false);
        assert_eq!(editor, "vim");
    }
}
