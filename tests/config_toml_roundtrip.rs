use pet::config::Config;

const FULL_FIXTURE: &str = r#"
[General]
  snippetfile = "/home/user/.config/pet/snippet.toml"
  snippetdirs = ["/home/user/snippets", "/more/snippets"]
  editor = "vim"
  column = 60
  selectcmd = "peco"
  backend = "gitlab"
  sortby = "-description"
  cmd = ["zsh", "-c"]
  color = true
  format = "[$description]: $command $tags"

[Gist]
  file_name = "pet-snippet.toml"
  access_token = "gist-token"
  gist_id = "abc123"
  public = true
  auto_sync = true

[GitLab]
  file_name = "pet-snippet.toml"
  access_token = "gitlab-token"
  url = "https://gitlab.example.com"
  id = "42"
  visibility = "internal"
  auto_sync = false
  skip_ssl = true

[GHEGist]
  base_url = "https://ghe.example.com"
  upload_url = "https://ghe.example.com/upload"
  file_name = "pet-snippet.toml"
  access_token = "ghe-token"
  gist_id = "xyz"
  public = false
  auto_sync = true
"#;

#[test]
fn full_config_round_trips_through_toml() {
    let cfg: Config = toml::from_str(FULL_FIXTURE).expect("parse full fixture");

    assert_eq!(
        cfg.general.snippetfile,
        "/home/user/.config/pet/snippet.toml"
    );
    assert_eq!(
        cfg.general.snippetdirs,
        vec![
            "/home/user/snippets".to_string(),
            "/more/snippets".to_string()
        ]
    );
    assert_eq!(cfg.general.column, 60);
    assert_eq!(cfg.general.selectcmd, "peco");
    assert_eq!(cfg.general.cmd, vec!["zsh".to_string(), "-c".to_string()]);
    assert!(cfg.general.color);
    assert_eq!(cfg.gist.access_token, "gist-token");
    assert!(cfg.gist.public);
    assert_eq!(cfg.gitlab.visibility, "internal");
    assert!(cfg.gitlab.skip_ssl);
    assert_eq!(cfg.ghe_gist.base_url, "https://ghe.example.com");

    let serialized = toml::to_string_pretty(&cfg).expect("serialize");
    let reparsed: Config = toml::from_str(&serialized).expect("reparse");
    assert_eq!(cfg, reparsed);
}

#[test]
fn minimal_config_gets_go_compatible_defaults() {
    let minimal = r#"
[General]
  snippetfile = "/home/user/.config/pet/snippet.toml"
"#;
    let cfg: Config = toml::from_str(minimal).expect("parse minimal fixture");

    assert_eq!(cfg.general.column, 40);
    assert_eq!(
        cfg.general.selectcmd,
        "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
    );
    assert_eq!(cfg.general.cmd, vec!["sh".to_string(), "-c".to_string()]);
    assert_eq!(cfg.general.format, "[$description]: $command $tags");
    assert_eq!(cfg.general.backend, "gist");
    assert!(cfg.general.snippetdirs.is_empty());
    assert!(!cfg.general.color);
    assert_eq!(cfg.gist, Default::default());
}

#[test]
fn load_creates_default_config_and_empty_snippet_file_when_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");

    let cfg = Config::load(&config_path).expect("load should create defaults");

    assert!(config_path.exists());
    let snippet_file = dir.path().join("snippet.toml");
    assert!(snippet_file.exists());
    assert_eq!(
        std::fs::read_to_string(&snippet_file).expect("read snippet.toml"),
        ""
    );
    assert_eq!(cfg.general.snippetfile, snippet_file.to_string_lossy());
    assert_eq!(cfg.general.column, 40);
    assert_eq!(cfg.gist.file_name, "pet-snippet.toml");
    assert_eq!(cfg.gitlab.visibility, "private");
    // Fresh installs get color on and the native picker selected by default; see
    // GeneralConfig::default's doc comments for why these differ from the fields'
    // own serde defaults (which stay Go-pet-compatible for configs missing the key).
    assert!(cfg.general.color);
    assert_eq!(cfg.general.selectcmd, "builtin");

    // Loading again should read back the same config, not re-create it.
    let reloaded = Config::load(&config_path).expect("reload existing config");
    assert_eq!(cfg, reloaded);
}
