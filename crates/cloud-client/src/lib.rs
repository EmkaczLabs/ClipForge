//! HTTP client for ClipForge cloud sharing server.

use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clipforge_common::{ClipForgeError, ClipId, Result};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

pub use clipforge_common::{ClipMetadata, CloudClient, CloudShareInfo};

/// Visibility level for a cloud-shared clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareVisibility {
    Public,
    Private,
}

/// Summary returned by list/upload/visibility endpoints on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteClipSummary {
    pub share_id: String,
    pub url: String,
    pub uploaded_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub clip_id: ClipId,
    pub title: String,
    pub visibility: ShareVisibility,
    pub size_bytes: u64,
}

impl From<RemoteClipSummary> for CloudShareInfo {
    fn from(s: RemoteClipSummary) -> Self {
        Self {
            share_id: s.share_id,
            url: s.url,
            uploaded_at: s.uploaded_at,
            expires_at: s.expires_at,
        }
    }
}

/// Response returned by `/api/v1/auth/register` and `/api/v1/auth/login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user_id: String,
    pub username: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// `reqwest`-based [`CloudClient`] implementation for ClipForge Server.
#[derive(Debug, Clone)]
pub struct HttpCloudClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl HttpCloudClient {
    /// Creates a new [`HttpCloudClient`] targeting `base_url` with an optional bearer token or API key.
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
        }
    }

    /// Returns a copy of this client configured with `token`.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Updates the bearer token / API key in place.
    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    /// Returns the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(tok) => req.bearer_auth(tok),
            None => req,
        }
    }

    /// Registers a new user account and returns the issued bearer token.
    pub async fn register(&self, username: &str, password: &str) -> Result<AuthResponse> {
        let url = format!("{}/api/v1/auth/register", self.base_url);
        let resp = self
            .client
            .post(url)
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("register request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Logs in with username and password and returns a bearer token.
    pub async fn login(&self, username: &str, password: &str) -> Result<AuthResponse> {
        let url = format!("{}/api/v1/auth/login", self.base_url);
        let resp = self
            .client
            .post(url)
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("login request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Streams a clip upload with explicit visibility (`public` or `private`).
    pub async fn upload_clip_with_visibility(
        &self,
        clip: &ClipMetadata,
        file_path: &Path,
        visibility: ShareVisibility,
    ) -> Result<RemoteClipSummary> {
        let file = tokio::fs::File::open(file_path).await?;
        let file_len = file.metadata().await?.len();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("clip.mp4")
            .to_string();

        let stream = ReaderStream::with_capacity(file, 64 * 1024);
        let body = reqwest::Body::wrap_stream(stream);
        let file_part = Part::stream_with_length(body, file_len)
            .file_name(file_name)
            .mime_str("application/octet-stream")
            .map_err(|e| ClipForgeError::Cloud(format!("invalid mime type: {e}")))?;

        let meta_json = serde_json::to_string(clip)
            .map_err(|e| ClipForgeError::Serialization(format!("serialize metadata: {e}")))?;
        let vis_str = match visibility {
            ShareVisibility::Public => "public",
            ShareVisibility::Private => "private",
        };

        let form = Form::new()
            .text("metadata", meta_json)
            .text("visibility", vis_str)
            .part("file", file_part);

        let url = format!("{}/api/v1/clips", self.base_url);
        let resp = self
            .apply_auth(self.client.post(url).multipart(form))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("upload request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Lists clips owned by the authenticated user.
    pub async fn list_clips(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<RemoteClipSummary>> {
        let mut url = format!("{}/api/v1/clips", self.base_url);
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={l}"));
        }
        if let Some(o) = offset {
            params.push(format!("offset={o}"));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("list clips failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Updates visibility (`public` or `private`) for a clip or share.
    pub async fn set_visibility(
        &self,
        share_or_clip_id: &str,
        visibility: ShareVisibility,
    ) -> Result<RemoteClipSummary> {
        let url = format!(
            "{}/api/v1/shares/{}/visibility",
            self.base_url, share_or_clip_id
        );
        let resp = self
            .apply_auth(self.client.patch(url))
            .json(&serde_json::json!({ "visibility": visibility }))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("set visibility failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Deletes a clip by clip ID or share ID.
    pub async fn delete_clip(&self, share_or_clip_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/clips/{}", self.base_url, share_or_clip_id);
        let resp = self
            .apply_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("delete clip failed: {e}")))?;

        check_empty_response(resp).await
    }
}

#[async_trait]
impl CloudClient for HttpCloudClient {
    async fn health_check(&self) -> Result<()> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("health check failed: {e}")))?;

        check_empty_response(resp).await
    }

    async fn upload_clip(&self, clip: &ClipMetadata, file_path: &Path) -> Result<CloudShareInfo> {
        let summary = self
            .upload_clip_with_visibility(clip, file_path, ShareVisibility::Public)
            .await?;
        Ok(summary.into())
    }

    async fn get_share(&self, share_id: &str) -> Result<CloudShareInfo> {
        let url = format!("{}/api/v1/shares/{}", self.base_url, share_id);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("get share failed: {e}")))?;

        parse_json_response(resp).await
    }

    async fn delete_share(&self, share_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/shares/{}", self.base_url, share_id);
        let resp = self
            .apply_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("delete share failed: {e}")))?;

        check_empty_response(resp).await
    }
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(map_http_error(status, &text));
    }
    resp.json::<T>()
        .await
        .map_err(|e| ClipForgeError::Serialization(format!("decode response: {e}")))
}

async fn check_empty_response(resp: reqwest::Response) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(map_http_error(status, &text));
    }
    Ok(())
}

fn map_http_error(status: reqwest::StatusCode, body: &str) -> ClipForgeError {
    if status == reqwest::StatusCode::NOT_FOUND {
        ClipForgeError::NotFound(format!("remote resource not found: {body}"))
    } else if status == reqwest::StatusCode::BAD_REQUEST {
        ClipForgeError::InvalidInput(format!("bad request: {body}"))
    } else {
        ClipForgeError::Cloud(format!("server returned {status}: {body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_trims_trailing_slash_and_sets_token() {
        let mut client = HttpCloudClient::new("https://clips.example.com/", None);
        assert_eq!(client.base_url(), "https://clips.example.com");
        assert!(client.token.is_none());

        client.set_token(Some("cf_tok_abc".to_string()));
        assert_eq!(client.token.as_deref(), Some("cf_tok_abc"));

        let cloned = client.with_token("cf_key_xyz");
        assert_eq!(cloned.token.as_deref(), Some("cf_key_xyz"));
    }

    #[test]
    fn remote_summary_converts_to_cloud_share_info() {
        let now = Utc::now();
        let summary = RemoteClipSummary {
            share_id: "abc123def456".to_string(),
            url: "https://clips.example.com/s/abc123def456".to_string(),
            uploaded_at: now,
            expires_at: None,
            clip_id: ClipId::new(),
            title: "Test Clip".to_string(),
            visibility: ShareVisibility::Public,
            size_bytes: 1024,
        };
        let info: CloudShareInfo = summary.into();
        assert_eq!(info.share_id, "abc123def456");
        assert_eq!(info.url, "https://clips.example.com/s/abc123def456");
        assert_eq!(info.uploaded_at, now);
    }
}
