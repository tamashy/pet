use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Result, bail};
use dialoguer::Confirm;
use owo_colors::{OwoColorize, Stream::Stdout};

use crate::config::Config;
use crate::error::SyncError;
use crate::gist::GistClient;
use crate::path::expand_absolute;
use crate::snippet::Snippets;

/// Resolve the GitHub access token: `[Gist] access_token` in config.toml first,
/// falling back to `github_token_env` (the caller passes `$GITHUB_TOKEN`) so a
/// user can keep the token out of a plaintext file if they'd rather.
fn resolve_access_token(
    config: &Config,
    github_token_env: Option<String>,
) -> Result<String, SyncError> {
    if !config.gist.access_token.is_empty() {
        return Ok(config.gist.access_token.clone());
    }
    github_token_env
        .filter(|t| !t.is_empty())
        .ok_or(SyncError::MissingAccessToken)
}

fn gist_file_name(config: &Config) -> &str {
    if config.gist.file_name.is_empty() {
        "pet-snippet.toml"
    } else {
        &config.gist.file_name
    }
}

/// `pet sync push`: upload the local snippet file's raw text to the configured
/// gist, creating it on the first push. Uploads the file exactly as it sits on
/// disk (not a re-serialization of `Snippets`) so push doesn't silently reformat
/// the user's file — this also makes push -> pull a byte-identical round trip.
pub fn run_push(config: &Config, config_path: &Path, client: &impl GistClient) -> Result<()> {
    resolve_access_token(config, std::env::var("GITHUB_TOKEN").ok())?;

    let snippet_path = expand_absolute(&config.general.snippetfile)?;
    let content = std::fs::read_to_string(&snippet_path)?;
    let file_name = gist_file_name(config);

    let info = if config.gist.gist_id.is_empty() {
        let info = client.create(file_name, &content, "pet snippets", config.gist.public)?;
        let mut updated = config.clone();
        updated.gist.gist_id = info.id.clone();
        updated.save(config_path)?;
        info
    } else {
        client.update(&config.gist.gist_id, file_name, &content)?
    };

    println!(
        "{} {}",
        "Pushed:".if_supports_color(Stdout, |t| t.bright_green()),
        info.html_url
    );
    Ok(())
}

/// `pet sync pull`: download the configured gist and overwrite the local
/// snippet file. `confirm` is injected (real implementation prompts
/// interactively; tests pass a canned answer) so the validate -> compare ->
/// conditional-write logic here is testable without a real terminal.
pub fn run_pull(
    config: &Config,
    client: &impl GistClient,
    yes: bool,
    confirm: impl FnOnce(usize, usize) -> Result<bool>,
) -> Result<()> {
    resolve_access_token(config, std::env::var("GITHUB_TOKEN").ok())?;

    if config.gist.gist_id.is_empty() {
        return Err(SyncError::MissingGistId.into());
    }
    let file_name = gist_file_name(config);

    let remote = client.get(&config.gist.gist_id)?;
    let remote_file = remote.file(&config.gist.gist_id, file_name)?;

    // Validate before touching anything on disk: a corrupt/unparseable gist
    // must never clobber a working local snippet file.
    let remote_snippets: Snippets = toml::from_str(&remote_file.content)
        .map_err(|source| SyncError::InvalidRemoteSnippets(Box::new(source)))?;

    let local_count = Snippets::load(&config.general, false)
        .map(|s| s.snippets.len())
        .unwrap_or(0);
    let remote_count = remote_snippets.snippets.len();

    if !yes && !confirm(local_count, remote_count)? {
        println!(
            "{}",
            "Cancelled.".if_supports_color(Stdout, |t| t.bright_yellow())
        );
        return Ok(());
    }

    let snippet_path = expand_absolute(&config.general.snippetfile)?;
    std::fs::write(&snippet_path, &remote_file.content)?;

    println!(
        "{} {local_count} local snippet(s) replaced with {remote_count} from the gist",
        "Pulled:".if_supports_color(Stdout, |t| t.bright_green()),
    );
    Ok(())
}

/// The real interactive confirmation for `run_pull`: prints a summary and prompts,
/// refusing to hang/error confusingly in a non-interactive session (mirrors
/// `cmd::new::scan`'s terminal check) — `pet sync pull` without `-y` in a script
/// should fail fast, not block forever on a prompt nobody can answer.
pub fn confirm_overwrite(local_count: usize, remote_count: usize) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!(
            "refusing to prompt for confirmation in a non-interactive session; pass -y/--yes to pull without confirming"
        );
    }
    println!("This will replace {local_count} local snippet(s) with {remote_count} from the gist.");
    Ok(Confirm::new()
        .with_prompt("Continue?")
        .default(false)
        .interact()?)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use tempfile::tempdir;

    use super::*;
    use crate::config::GistConfig;
    use crate::gist::{GistFile, GistInfo};

    #[derive(Default)]
    struct FakeGistClient {
        calls: RefCell<Vec<String>>,
        create_result: Option<GistInfo>,
        update_result: Option<GistInfo>,
        get_result: Option<GistInfo>,
    }

    impl GistClient for FakeGistClient {
        fn create(
            &self,
            _file_name: &str,
            _content: &str,
            _description: &str,
            _public: bool,
        ) -> Result<GistInfo, SyncError> {
            self.calls.borrow_mut().push("create".to_string());
            Ok(self.create_result.clone().expect("unexpected create call"))
        }

        fn update(
            &self,
            _gist_id: &str,
            _file_name: &str,
            _content: &str,
        ) -> Result<GistInfo, SyncError> {
            self.calls.borrow_mut().push("update".to_string());
            Ok(self.update_result.clone().expect("unexpected update call"))
        }

        fn get(&self, _gist_id: &str) -> Result<GistInfo, SyncError> {
            self.calls.borrow_mut().push("get".to_string());
            Ok(self.get_result.clone().expect("unexpected get call"))
        }
    }

    fn base_config(dir: &Path) -> Config {
        let snippetfile = dir.join("snippet.toml");
        std::fs::write(&snippetfile, "").unwrap();
        Config {
            general: crate::config::GeneralConfig {
                snippetfile: snippetfile.to_string_lossy().into_owned(),
                ..Default::default()
            },
            gist: GistConfig {
                access_token: "token".to_string(),
                file_name: "pet-snippet.toml".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn resolve_access_token_prefers_config_over_env() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.access_token = "from-config".to_string();
        let token = resolve_access_token(&config, Some("from-env".to_string())).unwrap();
        assert_eq!(token, "from-config");
    }

    #[test]
    fn resolve_access_token_falls_back_to_env() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.access_token = String::new();
        let token = resolve_access_token(&config, Some("from-env".to_string())).unwrap();
        assert_eq!(token, "from-env");
    }

    #[test]
    fn resolve_access_token_errors_when_both_empty() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.access_token = String::new();
        let err = resolve_access_token(&config, None).unwrap_err();
        assert!(matches!(err, SyncError::MissingAccessToken));
    }

    #[test]
    fn push_creates_a_gist_and_persists_the_returned_id() {
        let dir = tempdir().unwrap();
        let config = base_config(dir.path());
        let config_path = dir.path().join("config.toml");
        config.save(&config_path).unwrap();

        let client = FakeGistClient {
            create_result: Some(GistInfo {
                id: "new-id".to_string(),
                html_url: "https://gist.github.com/new-id".to_string(),
                files: HashMap::new(),
            }),
            ..Default::default()
        };

        run_push(&config, &config_path, &client).unwrap();

        assert_eq!(*client.calls.borrow(), vec!["create".to_string()]);
        let reloaded = Config::load(&config_path).unwrap();
        assert_eq!(reloaded.gist.gist_id, "new-id");
    }

    #[test]
    fn push_updates_an_existing_gist_without_rewriting_config() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.gist_id = "existing-id".to_string();
        let config_path = dir.path().join("config.toml");
        config.save(&config_path).unwrap();

        let client = FakeGistClient {
            update_result: Some(GistInfo {
                id: "existing-id".to_string(),
                html_url: "https://gist.github.com/existing-id".to_string(),
                files: HashMap::new(),
            }),
            ..Default::default()
        };

        run_push(&config, &config_path, &client).unwrap();

        assert_eq!(*client.calls.borrow(), vec!["update".to_string()]);
        let reloaded = Config::load(&config_path).unwrap();
        assert_eq!(reloaded.gist.gist_id, "existing-id");
    }

    fn gist_with_content(content: &str) -> GistInfo {
        let mut files = HashMap::new();
        files.insert(
            "pet-snippet.toml".to_string(),
            GistFile {
                content: content.to_string(),
            },
        );
        GistInfo {
            id: "existing-id".to_string(),
            html_url: "https://gist.github.com/existing-id".to_string(),
            files,
        }
    }

    #[test]
    fn pull_refuses_to_write_when_remote_content_is_invalid_toml() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.gist_id = "existing-id".to_string();
        std::fs::write(&config.general.snippetfile, "original content").unwrap();

        let client = FakeGistClient {
            get_result: Some(gist_with_content("not valid toml [[[")),
            ..Default::default()
        };

        let result = run_pull(&config, &client, true, |_, _| {
            panic!("confirm should not be called before validation succeeds")
        });

        assert!(result.is_err());
        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "original content");
    }

    #[test]
    fn pull_with_yes_skips_the_confirm_closure() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.gist_id = "existing-id".to_string();

        let client = FakeGistClient {
            get_result: Some(gist_with_content("[[snippets]]\ncommand = \"echo hi\"\n")),
            ..Default::default()
        };

        run_pull(&config, &client, true, |_, _| {
            panic!("confirm should not be called when yes=true")
        })
        .unwrap();

        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "[[snippets]]\ncommand = \"echo hi\"\n");
    }

    #[test]
    fn pull_declining_confirmation_leaves_the_local_file_untouched() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gist.gist_id = "existing-id".to_string();
        std::fs::write(&config.general.snippetfile, "original content").unwrap();

        let client = FakeGistClient {
            get_result: Some(gist_with_content("[[snippets]]\ncommand = \"echo hi\"\n")),
            ..Default::default()
        };

        run_pull(&config, &client, false, |_, _| Ok(false)).unwrap();

        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "original content");
    }

    #[test]
    fn pull_missing_gist_id_errors_before_any_network_call() {
        let dir = tempdir().unwrap();
        let config = base_config(dir.path());
        let client = FakeGistClient::default();

        let result = run_pull(&config, &client, true, |_, _| {
            panic!("confirm should not be called")
        });

        assert!(result.is_err());
        assert!(client.calls.borrow().is_empty());
    }

    #[test]
    fn gist_info_file_lookup_lists_found_files_when_missing() {
        let info = gist_with_content("content");
        let err = info.file("existing-id", "wrong-name.toml").unwrap_err();
        match err {
            SyncError::FileNotFoundInGist {
                gist_id,
                file_name,
                found,
            } => {
                assert_eq!(gist_id, "existing-id");
                assert_eq!(file_name, "wrong-name.toml");
                assert_eq!(found, vec!["pet-snippet.toml".to_string()]);
            }
            other => panic!("expected FileNotFoundInGist, got {other:?}"),
        }
    }
}
