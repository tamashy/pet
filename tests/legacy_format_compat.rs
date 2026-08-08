use pet::config::Config;
use pet::snippet::Snippets;

/// Real-world config.toml from an existing pet install (pelletier/go-toml emits the
/// raw Go struct field name for any field without an explicit `toml:"..."` tag, so
/// most of `[General]` ends up PascalCase while explicitly-tagged fields like
/// `access_token` stay lowercase). See config.rs / snippet.rs doc comments.
const LEGACY_CONFIG: &str = r#"
[GHEGist]
  Public = false
  access_token = ""
  auto_sync = false
  base_url = ""
  file_name = ""
  gist_id = ""
  upload_url = ""

[General]
  Backend = "gist"
  Cmd = []
  Color = false
  Column = 40
  Editor = "vim"
  Format = "[$description]: $command $tags"
  SelectCmd = "fzf --ansi --layout=reverse --border --height=90% --pointer=* --cycle --prompt=Snippets:"
  SnippetDirs = []
  SnippetFile = "/home/user/.config/pet/snippet.toml"
  SortBy = ""

[Gist]
  Public = false
  access_token = ""
  auto_sync = false
  file_name = "pet-snippet.toml"
  gist_id = ""

[GitLab]
  ID = ""
  Url = ""
  Visibility = "private"
  access_token = ""
  auto_sync = false
  file_name = "pet-snippet.toml"
  skip_ssl = false
"#;

#[test]
fn parses_legacy_pascal_case_config() {
    let cfg: Config = toml::from_str(LEGACY_CONFIG).expect("parse legacy config");

    assert_eq!(cfg.general.editor, "vim");
    assert_eq!(
        cfg.general.snippetfile,
        "/home/user/.config/pet/snippet.toml"
    );
    assert_eq!(cfg.general.backend, "gist");
    assert_eq!(cfg.general.column, 40);
    assert_eq!(cfg.gitlab.url, "");
    assert_eq!(cfg.gitlab.id, "");
    assert_eq!(cfg.gitlab.visibility, "private");
    assert!(!cfg.gist.public);
    assert!(!cfg.ghe_gist.public);
}

#[test]
fn parses_legacy_pascal_case_snippet_file() {
    let dir = tempfile::tempdir().unwrap();
    let snippet_file = dir.path().join("snippet.toml");
    std::fs::write(
        &snippet_file,
        "[[snippets]]\nDescription = \"legacy\"\ncommand = \"echo legacy\"\nTag = [\"x\"]\n",
    )
    .unwrap();

    let general = pet::config::GeneralConfig {
        snippetfile: snippet_file.to_string_lossy().into_owned(),
        ..pet::config::GeneralConfig::default()
    };
    let snippets = Snippets::load(&general, false).unwrap();

    assert_eq!(snippets.snippets.len(), 1);
    assert_eq!(snippets.snippets[0].description, "legacy");
    assert_eq!(snippets.snippets[0].tag, vec!["x".to_string()]);
}
