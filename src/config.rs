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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub snippetfile: String,
    #[serde(default)]
    pub snippetdirs: Vec<String>,
    #[serde(default)]
    pub editor: String,
    #[serde(default = "default_column")]
    pub column: i32,
    #[serde(default = "default_selectcmd")]
    pub selectcmd: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub sortby: String,
    #[serde(default = "default_cmd")]
    pub cmd: Vec<String>,
    #[serde(default)]
    pub color: bool,
    #[serde(default = "default_format")]
    pub format: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            snippetfile: String::new(),
            snippetdirs: Vec::new(),
            editor: String::new(),
            column: default_column(),
            selectcmd: default_selectcmd(),
            backend: default_backend(),
            sortby: String::new(),
            cmd: default_cmd(),
            color: false,
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
    #[serde(default)]
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
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
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
    #[serde(default)]
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

        let serialized = toml::to_string_pretty(&cfg)?;
        std::fs::write(path, serialized).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(cfg)
    }
}

fn default_editor() -> String {
    if let Ok(editor) = std::env::var("EDITOR")
        && !editor.is_empty()
    {
        return editor;
    }
    if cfg!(windows) {
        return String::new();
    }
    if is_command_available("sensible-editor") {
        "sensible-editor".to_string()
    } else {
        "vim".to_string()
    }
}

fn is_command_available(name: &str) -> bool {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
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
