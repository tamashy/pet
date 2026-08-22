use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::SyncError;

/// What `cmd::sync` needs from the GitHub Gist API, abstracted behind a trait so
/// the push/pull decision logic (create-vs-update, overwrite confirmation, count
/// comparison) is unit-testable against a fake, while `GistApiClient` (the real
/// ureq-backed implementation) stays a thin, untested adapter — the same boundary
/// already drawn around `shell::spawn_inherit` and the `arboard` clipboard calls.
pub trait GistClient {
    /// `POST /gists` — create a new gist containing one file.
    fn create(
        &self,
        file_name: &str,
        content: &str,
        description: &str,
        public: bool,
    ) -> Result<GistInfo, SyncError>;

    /// `PATCH /gists/{id}` — replace one file's content in an existing gist.
    /// Files not named in the request are left untouched, so this is safe to use
    /// even on a gist that also holds unrelated files.
    fn update(&self, gist_id: &str, file_name: &str, content: &str) -> Result<GistInfo, SyncError>;

    /// `GET /gists/{id}` — fetch a gist's metadata and file contents.
    fn get(&self, gist_id: &str) -> Result<GistInfo, SyncError>;
}

/// The pieces of a Gist API response push/pull actually need — a normalized view
/// over the create/update/get responses (they're shaped identically).
#[derive(Debug, Clone)]
pub struct GistInfo {
    pub id: String,
    pub html_url: String,
    pub files: HashMap<String, GistFile>,
}

#[derive(Debug, Clone)]
pub struct GistFile {
    pub content: String,
}

impl GistInfo {
    /// Look up `file_name` among this gist's files, with an error that lists what
    /// *was* found so a misconfigured `file_name` is easy to diagnose.
    pub fn file<'a>(&'a self, gist_id: &str, file_name: &str) -> Result<&'a GistFile, SyncError> {
        self.files
            .get(file_name)
            .ok_or_else(|| SyncError::FileNotFoundInGist {
                gist_id: gist_id.to_string(),
                file_name: file_name.to_string(),
                found: self.files.keys().cloned().collect(),
            })
    }
}

const GITHUB_API_BASE: &str = "https://api.github.com";

/// Real GitHub Gist API v3 client, backed by `ureq` (blocking, no async runtime —
/// matches the rest of this codebase). `base_url` defaults to `GITHUB_API_BASE`
/// but can be overridden via the `GIST_API_BASE_URL` env var (wired in `main.rs`)
/// to point at a mock server, e.g. for future integration tests.
pub struct GistApiClient {
    base_url: String,
    access_token: String,
}

impl GistApiClient {
    pub fn new(access_token: String) -> Self {
        Self::with_base_url(access_token, GITHUB_API_BASE.to_string())
    }

    pub fn with_base_url(access_token: String, base_url: String) -> Self {
        GistApiClient {
            base_url,
            access_token,
        }
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        ureq::request(method, &format!("{}{}", self.base_url, path))
            .set("Authorization", &format!("Bearer {}", self.access_token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "pet-cli")
    }

    fn send(
        resp: Result<ureq::Response, ureq::Error>,
        gist_id_for_404: Option<&str>,
    ) -> Result<GistInfo, SyncError> {
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(401, _)) => return Err(SyncError::Unauthorized),
            Err(ureq::Error::Status(404, _)) => {
                return Err(match gist_id_for_404 {
                    Some(id) => SyncError::GistNotFound(id.to_string()),
                    None => SyncError::UnexpectedStatus {
                        status: 404,
                        body: String::new(),
                    },
                });
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                return Err(SyncError::UnexpectedStatus { status, body });
            }
            Err(err @ ureq::Error::Transport(_)) => return Err(SyncError::Request(Box::new(err))),
        };

        let body: GistApiResponse = resp.into_json()?;
        body.try_into()
    }
}

impl GistClient for GistApiClient {
    fn create(
        &self,
        file_name: &str,
        content: &str,
        description: &str,
        public: bool,
    ) -> Result<GistInfo, SyncError> {
        let body = GistCreateBody {
            description: description.to_string(),
            public,
            files: HashMap::from([(
                file_name.to_string(),
                GistFileBody {
                    content: content.to_string(),
                },
            )]),
        };
        let resp = self
            .request("POST", "/gists")
            .send_json(serde_json::to_value(&body)?);
        Self::send(resp, None)
    }

    fn update(&self, gist_id: &str, file_name: &str, content: &str) -> Result<GistInfo, SyncError> {
        let body = GistUpdateBody {
            files: HashMap::from([(
                file_name.to_string(),
                GistFileBody {
                    content: content.to_string(),
                },
            )]),
        };
        let resp = self
            .request("PATCH", &format!("/gists/{gist_id}"))
            .send_json(serde_json::to_value(&body)?);
        Self::send(resp, Some(gist_id))
    }

    fn get(&self, gist_id: &str) -> Result<GistInfo, SyncError> {
        let resp = self.request("GET", &format!("/gists/{gist_id}")).call();
        Self::send(resp, Some(gist_id))
    }
}

#[derive(Serialize)]
struct GistCreateBody {
    description: String,
    public: bool,
    files: HashMap<String, GistFileBody>,
}

#[derive(Serialize)]
struct GistUpdateBody {
    files: HashMap<String, GistFileBody>,
}

#[derive(Serialize)]
struct GistFileBody {
    content: String,
}

#[derive(Deserialize)]
struct GistApiResponse {
    id: String,
    html_url: String,
    files: HashMap<String, GistApiFile>,
}

#[derive(Deserialize)]
struct GistApiFile {
    content: String,
    #[serde(default)]
    truncated: bool,
}

impl TryFrom<GistApiResponse> for GistInfo {
    type Error = SyncError;

    fn try_from(resp: GistApiResponse) -> Result<Self, SyncError> {
        if let Some((name, _)) = resp.files.iter().find(|(_, f)| f.truncated) {
            return Err(SyncError::Truncated(name.clone()));
        }
        Ok(GistInfo {
            id: resp.id,
            html_url: resp.html_url,
            files: resp
                .files
                .into_iter()
                .map(|(name, f)| (name, GistFile { content: f.content }))
                .collect(),
        })
    }
}
