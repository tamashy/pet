use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to determine config directory")]
    NoConfigDir,
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write config file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SnippetError {
    #[error(
        "snippet file not found: {0}\nPlease run 'pet configure' and provide a correct file path, or remove this if you only want to provide snippetdirs instead"
    )]
    SnippetFileNotFound(PathBuf),
    #[error("snippet directory not found: {0}")]
    SnippetDirNotFound(PathBuf),
    #[error("failed to read snippet file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write snippet file {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse snippet file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("failed to serialize snippets: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("snippet [{0}] already exists")]
    DuplicateDescription(String),
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(
        "no Gist access token configured\nPlease run 'pet configure' and set access_token under [Gist] (a GitHub personal access token with the 'gist' scope), or set the GITHUB_TOKEN environment variable"
    )]
    MissingAccessToken,
    #[error(
        "no gist_id configured, nothing to pull yet\nRun 'pet sync push' first to create a gist, or set gist_id under [Gist] in config.toml if you already have one"
    )]
    MissingGistId,
    #[error(
        "GitHub rejected the access token (401 Unauthorized) — check access_token under [Gist] (needs the 'gist' scope) or GITHUB_TOKEN"
    )]
    Unauthorized,
    #[error(
        "gist {0} not found (404) — check gist_id under [Gist], or that the token has access to it"
    )]
    GistNotFound(String),
    #[error(
        "gist {gist_id} has no file named \"{file_name}\" (found: {found:?}) — check file_name under [Gist]"
    )]
    FileNotFoundInGist {
        gist_id: String,
        file_name: String,
        found: Vec<String>,
    },
    #[error(
        "gist file \"{0}\" was truncated by GitHub's API (the file is too large to fetch in full) — refusing to pull a partial snippet file"
    )]
    Truncated(String),
    #[error("network request to GitHub failed: {0}")]
    Request(#[from] Box<ureq::Error>),
    #[error("failed to read response body from GitHub: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse GitHub's response as JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "the gist's snippet file failed to parse as valid TOML, refusing to overwrite your local snippets: {0}"
    )]
    InvalidRemoteSnippets(#[source] Box<toml::de::Error>),
    #[error("GitHub returned an unexpected response (HTTP {status}): {body}")]
    UnexpectedStatus { status: u16, body: String },
}
