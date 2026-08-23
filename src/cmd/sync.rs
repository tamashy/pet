use std::io::IsTerminal;
use std::path::Path;

use anyhow::{Result, bail};
use dialoguer::Confirm;
use owo_colors::{OwoColorize, Stream::Stdout};

use crate::config::Config;
use crate::error::SyncError;
use crate::gist::GistClient;
use crate::gitlab::GitLabClient;
use crate::path::expand_absolute;
use crate::snippet::Snippets;

/// Resolve an access token: the value already configured (e.g. `[Gist]
/// access_token`) first, falling back to `env_token` (the caller passes an
/// env var like `$GITHUB_TOKEN`/`$GITLAB_TOKEN`) so a user can keep the token
/// out of a plaintext file if they'd rather. `missing_err` names which
/// backend's error to raise if both are empty.
fn resolve_access_token(
    configured: &str,
    env_token: Option<String>,
    missing_err: SyncError,
) -> Result<String, SyncError> {
    if !configured.is_empty() {
        return Ok(configured.to_string());
    }
    env_token.filter(|t| !t.is_empty()).ok_or(missing_err)
}

fn gist_file_name(config: &Config) -> &str {
    if config.gist.file_name.is_empty() {
        "pet-snippet.toml"
    } else {
        &config.gist.file_name
    }
}

fn gitlab_file_name(config: &Config) -> &str {
    if config.gitlab.file_name.is_empty() {
        "pet-snippet.toml"
    } else {
        &config.gitlab.file_name
    }
}

fn gitlab_visibility(config: &Config) -> &str {
    if config.gitlab.visibility.is_empty() {
        "private"
    } else {
        &config.gitlab.visibility
    }
}

/// `pet sync push` (Gist backend): upload the local snippet file's raw text
/// to the configured gist, creating it on the first push. Uploads the file
/// exactly as it sits on disk (not a re-serialization of `Snippets`) so push
/// doesn't silently reformat the user's file — this also makes push -> pull
/// a byte-identical round trip.
pub fn run_push_gist(config: &Config, config_path: &Path, client: &impl GistClient) -> Result<()> {
    resolve_access_token(
        &config.gist.access_token,
        std::env::var("GITHUB_TOKEN").ok(),
        SyncError::MissingAccessToken,
    )?;

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

/// `pet sync pull` (Gist backend): download the configured gist and overwrite
/// the local snippet file. See `finish_pull` for the shared validate/confirm/
/// write tail.
pub fn run_pull_gist(
    config: &Config,
    client: &impl GistClient,
    yes: bool,
    confirm: impl FnOnce(usize, usize) -> Result<bool>,
) -> Result<()> {
    resolve_access_token(
        &config.gist.access_token,
        std::env::var("GITHUB_TOKEN").ok(),
        SyncError::MissingAccessToken,
    )?;

    if config.gist.gist_id.is_empty() {
        return Err(SyncError::MissingGistId.into());
    }
    let file_name = gist_file_name(config);

    let remote = client.get(&config.gist.gist_id)?;
    let remote_file = remote.file(&config.gist.gist_id, file_name)?;

    finish_pull(config, &remote_file.content, yes, confirm)
}

/// `pet sync push` (GitLab backend): upload the local snippet file's raw text
/// to the configured GitLab personal snippet, creating it on the first push.
pub fn run_push_gitlab(
    config: &Config,
    config_path: &Path,
    client: &impl GitLabClient,
) -> Result<()> {
    resolve_access_token(
        &config.gitlab.access_token,
        std::env::var("GITLAB_TOKEN").ok(),
        SyncError::GitLabMissingAccessToken,
    )?;

    let snippet_path = expand_absolute(&config.general.snippetfile)?;
    let content = std::fs::read_to_string(&snippet_path)?;
    let file_name = gitlab_file_name(config);
    let visibility = gitlab_visibility(config);

    let info = if config.gitlab.id.is_empty() {
        let info = client.create(file_name, &content, "pet snippets", visibility)?;
        let mut updated = config.clone();
        updated.gitlab.id = info.id.clone();
        updated.save(config_path)?;
        info
    } else {
        client.update(&config.gitlab.id, file_name, &content)?
    };

    println!(
        "{} {}",
        "Pushed:".if_supports_color(Stdout, |t| t.bright_green()),
        info.web_url
    );
    Ok(())
}

/// `pet sync pull` (GitLab backend): download the configured snippet and
/// overwrite the local snippet file. See `finish_pull` for the shared
/// validate/confirm/write tail.
pub fn run_pull_gitlab(
    config: &Config,
    client: &impl GitLabClient,
    yes: bool,
    confirm: impl FnOnce(usize, usize) -> Result<bool>,
) -> Result<()> {
    resolve_access_token(
        &config.gitlab.access_token,
        std::env::var("GITLAB_TOKEN").ok(),
        SyncError::GitLabMissingAccessToken,
    )?;

    if config.gitlab.id.is_empty() {
        return Err(SyncError::GitLabMissingId.into());
    }
    let file_name = gitlab_file_name(config);
    let content = client.get(&config.gitlab.id, file_name)?;

    finish_pull(config, &content, yes, confirm)
}

/// The pull logic shared by every backend once the remote content has been
/// fetched: validate it parses as `Snippets` TOML *before touching disk*,
/// compare local/remote counts, confirm (unless `yes`), then write. `confirm`
/// is injected (real implementation prompts interactively; tests pass a
/// canned answer) so this is testable without a real terminal.
fn finish_pull(
    config: &Config,
    remote_content: &str,
    yes: bool,
    confirm: impl FnOnce(usize, usize) -> Result<bool>,
) -> Result<()> {
    let remote_snippets: Snippets = toml::from_str(remote_content)
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
    std::fs::write(&snippet_path, remote_content)?;

    println!(
        "{} {local_count} local snippet(s) replaced with {remote_count} from the remote",
        "Pulled:".if_supports_color(Stdout, |t| t.bright_green()),
    );
    Ok(())
}

/// The real interactive confirmation for pull: prints a summary and prompts,
/// refusing to hang/error confusingly in a non-interactive session (mirrors
/// `cmd::new::scan`'s terminal check) — `pet sync pull` without `-y` in a
/// script should fail fast, not block forever on a prompt nobody can answer.
pub fn confirm_overwrite(local_count: usize, remote_count: usize) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        bail!(
            "refusing to prompt for confirmation in a non-interactive session; pass -y/--yes to pull without confirming"
        );
    }
    println!(
        "This will replace {local_count} local snippet(s) with {remote_count} from the remote."
    );
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
    use crate::config::{GistConfig, GitLabConfig};
    use crate::gist::{GistFile, GistInfo};
    use crate::gitlab::SnippetInfo;

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

    #[derive(Default)]
    struct FakeGitLabClient {
        calls: RefCell<Vec<String>>,
        create_result: Option<SnippetInfo>,
        update_result: Option<SnippetInfo>,
        get_result: Option<String>,
    }

    impl GitLabClient for FakeGitLabClient {
        fn create(
            &self,
            _file_name: &str,
            _content: &str,
            _title: &str,
            _visibility: &str,
        ) -> Result<SnippetInfo, SyncError> {
            self.calls.borrow_mut().push("create".to_string());
            Ok(self.create_result.clone().expect("unexpected create call"))
        }

        fn update(
            &self,
            _id: &str,
            _file_name: &str,
            _content: &str,
        ) -> Result<SnippetInfo, SyncError> {
            self.calls.borrow_mut().push("update".to_string());
            Ok(self.update_result.clone().expect("unexpected update call"))
        }

        fn get(&self, _id: &str, _file_name: &str) -> Result<String, SyncError> {
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
            gitlab: GitLabConfig {
                access_token: "token".to_string(),
                file_name: "pet-snippet.toml".to_string(),
                visibility: "private".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn resolve_access_token_prefers_configured_over_env() {
        let token = resolve_access_token(
            "from-config",
            Some("from-env".to_string()),
            SyncError::MissingAccessToken,
        )
        .unwrap();
        assert_eq!(token, "from-config");
    }

    #[test]
    fn resolve_access_token_falls_back_to_env() {
        let token = resolve_access_token(
            "",
            Some("from-env".to_string()),
            SyncError::MissingAccessToken,
        )
        .unwrap();
        assert_eq!(token, "from-env");
    }

    #[test]
    fn resolve_access_token_errors_when_both_empty() {
        let err = resolve_access_token("", None, SyncError::MissingAccessToken).unwrap_err();
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

        run_push_gist(&config, &config_path, &client).unwrap();

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

        run_push_gist(&config, &config_path, &client).unwrap();

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

        let result = run_pull_gist(&config, &client, true, |_, _| {
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

        run_pull_gist(&config, &client, true, |_, _| {
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

        run_pull_gist(&config, &client, false, |_, _| Ok(false)).unwrap();

        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "original content");
    }

    #[test]
    fn pull_missing_gist_id_errors_before_any_network_call() {
        let dir = tempdir().unwrap();
        let config = base_config(dir.path());
        let client = FakeGistClient::default();

        let result = run_pull_gist(&config, &client, true, |_, _| {
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

    #[test]
    fn gitlab_push_creates_a_snippet_and_persists_the_returned_id() {
        let dir = tempdir().unwrap();
        let config = base_config(dir.path());
        let config_path = dir.path().join("config.toml");
        config.save(&config_path).unwrap();

        let client = FakeGitLabClient {
            create_result: Some(SnippetInfo {
                id: "42".to_string(),
                web_url: "https://gitlab.com/-/snippets/42".to_string(),
            }),
            ..Default::default()
        };

        run_push_gitlab(&config, &config_path, &client).unwrap();

        assert_eq!(*client.calls.borrow(), vec!["create".to_string()]);
        let reloaded = Config::load(&config_path).unwrap();
        assert_eq!(reloaded.gitlab.id, "42");
    }

    #[test]
    fn gitlab_push_updates_an_existing_snippet_without_rewriting_config() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gitlab.id = "42".to_string();
        let config_path = dir.path().join("config.toml");
        config.save(&config_path).unwrap();

        let client = FakeGitLabClient {
            update_result: Some(SnippetInfo {
                id: "42".to_string(),
                web_url: "https://gitlab.com/-/snippets/42".to_string(),
            }),
            ..Default::default()
        };

        run_push_gitlab(&config, &config_path, &client).unwrap();

        assert_eq!(*client.calls.borrow(), vec!["update".to_string()]);
        let reloaded = Config::load(&config_path).unwrap();
        assert_eq!(reloaded.gitlab.id, "42");
    }

    #[test]
    fn gitlab_pull_refuses_to_write_when_remote_content_is_invalid_toml() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gitlab.id = "42".to_string();
        std::fs::write(&config.general.snippetfile, "original content").unwrap();

        let client = FakeGitLabClient {
            get_result: Some("not valid toml [[[".to_string()),
            ..Default::default()
        };

        let result = run_pull_gitlab(&config, &client, true, |_, _| {
            panic!("confirm should not be called before validation succeeds")
        });

        assert!(result.is_err());
        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "original content");
    }

    #[test]
    fn gitlab_pull_with_yes_skips_the_confirm_closure() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gitlab.id = "42".to_string();

        let client = FakeGitLabClient {
            get_result: Some("[[snippets]]\ncommand = \"echo hi\"\n".to_string()),
            ..Default::default()
        };

        run_pull_gitlab(&config, &client, true, |_, _| {
            panic!("confirm should not be called when yes=true")
        })
        .unwrap();

        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "[[snippets]]\ncommand = \"echo hi\"\n");
    }

    #[test]
    fn gitlab_pull_declining_confirmation_leaves_the_local_file_untouched() {
        let dir = tempdir().unwrap();
        let mut config = base_config(dir.path());
        config.gitlab.id = "42".to_string();
        std::fs::write(&config.general.snippetfile, "original content").unwrap();

        let client = FakeGitLabClient {
            get_result: Some("[[snippets]]\ncommand = \"echo hi\"\n".to_string()),
            ..Default::default()
        };

        run_pull_gitlab(&config, &client, false, |_, _| Ok(false)).unwrap();

        let on_disk = std::fs::read_to_string(&config.general.snippetfile).unwrap();
        assert_eq!(on_disk, "original content");
    }

    #[test]
    fn gitlab_pull_missing_id_errors_before_any_network_call() {
        let dir = tempdir().unwrap();
        let config = base_config(dir.path());
        let client = FakeGitLabClient::default();

        let result = run_pull_gitlab(&config, &client, true, |_, _| {
            panic!("confirm should not be called")
        });

        assert!(result.is_err());
        assert!(client.calls.borrow().is_empty());
    }
}
