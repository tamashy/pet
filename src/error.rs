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
