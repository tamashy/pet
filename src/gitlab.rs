use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::SyncError;

/// What `cmd::sync` needs from the GitLab Snippets API, abstracted behind a
/// trait so the push/pull decision logic is unit-testable against a fake,
/// mirroring `gist::GistClient`.
pub trait GitLabClient {
    /// `POST /api/v4/snippets` — create a new (single-file) personal snippet.
    fn create(
        &self,
        file_name: &str,
        content: &str,
        title: &str,
        visibility: &str,
    ) -> Result<SnippetInfo, SyncError>;

    /// `PUT /api/v4/snippets/:id` — replace the file's content in an existing
    /// snippet.
    fn update(&self, id: &str, file_name: &str, content: &str) -> Result<SnippetInfo, SyncError>;

    /// Fetch a snippet's content by id. Checks the snippet's file list is
    /// exactly `[file_name]` first (a snippet hand-edited into multiple files
    /// via the GitLab web UI would otherwise silently return the wrong
    /// content, since `GET .../raw` has no file-selection concept for
    /// single-file requests) before fetching the raw content.
    fn get(&self, id: &str, file_name: &str) -> Result<String, SyncError>;
}

#[derive(Debug, Clone)]
pub struct SnippetInfo {
    pub id: String,
    pub web_url: String,
}

const GITLAB_DEFAULT_BASE: &str = "https://gitlab.com";

/// Real GitLab Snippets API v4 client, backed by `ureq`. `base_url` defaults
/// to `GITLAB_DEFAULT_BASE` but honors `[GitLab] url` for self-hosted
/// instances. When `skip_ssl` is true, TLS certificate verification is
/// disabled for this client's requests — see `insecure_agent` below.
pub struct GitLabApiClient {
    agent: ureq::Agent,
    base_url: String,
    access_token: String,
}

impl GitLabApiClient {
    pub fn new(access_token: String, base_url: String, skip_ssl: bool) -> Self {
        let base_url = if base_url.is_empty() {
            GITLAB_DEFAULT_BASE.to_string()
        } else {
            base_url.trim_end_matches('/').to_string()
        };

        let agent = if skip_ssl {
            eprintln!(
                "warning: skip_ssl is enabled under [GitLab] — TLS certificate verification is disabled for this request"
            );
            insecure_agent()
        } else {
            ureq::AgentBuilder::new().build()
        };

        GitLabApiClient {
            agent,
            base_url,
            access_token,
        }
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        self.agent
            .request(method, &format!("{}/api/v4{}", self.base_url, path))
            .set("PRIVATE-TOKEN", &self.access_token)
            .set("Accept", "application/json")
            .set("User-Agent", "pet-cli")
    }

    fn send(
        resp: Result<ureq::Response, ureq::Error>,
        id_for_404: Option<&str>,
    ) -> Result<GitLabApiResponse, SyncError> {
        let resp = match resp {
            Ok(r) => r,
            Err(ureq::Error::Status(401, _)) => return Err(SyncError::GitLabUnauthorized),
            Err(ureq::Error::Status(404, _)) => {
                return Err(match id_for_404 {
                    Some(id) => SyncError::GitLabSnippetNotFound(id.to_string()),
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

        Ok(resp.into_json()?)
    }
}

impl GitLabClient for GitLabApiClient {
    fn create(
        &self,
        file_name: &str,
        content: &str,
        title: &str,
        visibility: &str,
    ) -> Result<SnippetInfo, SyncError> {
        let body = GitLabCreateBody {
            title: title.to_string(),
            visibility: visibility.to_string(),
            files: vec![GitLabCreateFile {
                file_path: file_name.to_string(),
                content: content.to_string(),
            }],
        };
        let resp = self
            .request("POST", "/snippets")
            .send_json(serde_json::to_value(&body)?);
        Self::send(resp, None).map(Into::into)
    }

    fn update(&self, id: &str, file_name: &str, content: &str) -> Result<SnippetInfo, SyncError> {
        let body = GitLabUpdateBody {
            files: vec![GitLabUpdateFile {
                action: "update".to_string(),
                file_path: file_name.to_string(),
                content: content.to_string(),
            }],
        };
        let resp = self
            .request("PUT", &format!("/snippets/{id}"))
            .send_json(serde_json::to_value(&body)?);
        Self::send(resp, Some(id)).map(Into::into)
    }

    fn get(&self, id: &str, file_name: &str) -> Result<String, SyncError> {
        let resp = self.request("GET", &format!("/snippets/{id}")).call();
        let meta = Self::send(resp, Some(id))?;

        let paths: Vec<String> = meta.files.iter().map(|f| f.path.clone()).collect();
        if paths != vec![file_name.to_string()] {
            return Err(SyncError::GitLabFileNameMismatch {
                id: id.to_string(),
                expected: file_name.to_string(),
                found: paths,
            });
        }

        let raw = match self.request("GET", &format!("/snippets/{id}/raw")).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => {
                return Err(SyncError::GitLabSnippetNotFound(id.to_string()));
            }
            Err(ureq::Error::Status(status, response)) => {
                let body = response.into_string().unwrap_or_default();
                return Err(SyncError::UnexpectedStatus { status, body });
            }
            Err(err @ ureq::Error::Transport(_)) => return Err(SyncError::Request(Box::new(err))),
        };
        Ok(raw.into_string()?)
    }
}

#[derive(Serialize)]
struct GitLabCreateBody {
    title: String,
    visibility: String,
    files: Vec<GitLabCreateFile>,
}

#[derive(Serialize)]
struct GitLabCreateFile {
    file_path: String,
    content: String,
}

#[derive(Serialize)]
struct GitLabUpdateBody {
    files: Vec<GitLabUpdateFile>,
}

#[derive(Serialize)]
struct GitLabUpdateFile {
    action: String,
    file_path: String,
    content: String,
}

#[derive(Deserialize)]
struct GitLabApiResponse {
    id: serde_json::Number,
    web_url: String,
    // Only `get()`'s metadata call actually needs this (for the file-name
    // mismatch check below); create/update responses are never inspected for
    // it, so default to empty rather than hard-failing JSON parsing over a
    // field this code doesn't otherwise use.
    #[serde(default)]
    files: Vec<GitLabApiFile>,
}

#[derive(Deserialize)]
struct GitLabApiFile {
    path: String,
}

impl From<GitLabApiResponse> for SnippetInfo {
    fn from(resp: GitLabApiResponse) -> Self {
        SnippetInfo {
            id: resp.id.to_string(),
            web_url: resp.web_url,
        }
    }
}

/// A `ureq::Agent` with TLS certificate verification disabled, for `[GitLab]
/// skip_ssl = true` against self-hosted instances with self-signed certs.
/// Untested (would need a self-signed-cert test fixture to exercise for
/// real) — same accepted boundary as the rest of this module's real network
/// calls.
fn insecure_agent() -> ureq::Agent {
    let tls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertVerifier))
        .with_no_client_auth();

    ureq::AgentBuilder::new()
        .tls_config(Arc::new(tls_config))
        .build()
}

#[derive(Debug)]
struct NoCertVerifier;

impl rustls::client::danger::ServerCertVerifier for NoCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
