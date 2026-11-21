//! HTTP client for ClipForge Server (`/api/v1`).

use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clipforge_common::{ClipForgeError, ClipId, Result};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub use clipforge_common::{AudioTrackInfo, ClipMetadata, CloudClient, CloudShareInfo, GameInfo};

/// Visibility level for a cloud-shared clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShareVisibility {
    #[default]
    Public,
    Private,
}

/// User role on the ClipForge server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    #[default]
    User,
    Admin,
}

/// Structured error payload returned by `/api/v1` endpoints on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorDto {
    pub error: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Health check response returned by `GET /health` and `GET /api/v1/health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthDto {
    pub status: String,
    pub version: String,
    pub db_ok: bool,
    pub storage_ok: bool,
    pub uptime_secs: u64,
}

/// Storage quota usage and limits for a user account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaDto {
    pub used_bytes: u64,
    pub limit_bytes: u64,
    pub remaining_bytes: u64,
    pub clip_count: u64,
}

/// Public server capabilities and configuration summary (`GET /api/v1/server/info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfoDto {
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub storage_backend: String,
    pub max_upload_bytes: u64,
    pub chunk_size_bytes: u64,
    pub registration_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
}

/// Authenticated user profile (`GET /api/v1/auth/me`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub role: UserRole,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaDto>,
}

fn default_true() -> bool {
    true
}

/// Response returned by `/api/v1/auth/register` and `/api/v1/auth/login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user_id: String,
    pub username: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<UserInfo>,
}

/// Summary of an existing API token (`GET /api/v1/auth/tokens`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiTokenInfo {
    pub id: String,
    pub name: String,
    pub prefix: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Request body for `POST /api/v1/auth/tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<u32>,
}

/// Response returned when creating an API token (`POST /api/v1/auth/tokens`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTokenResponse {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub token: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Full representation of a clip stored on the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipDto {
    pub id: Uuid,
    pub owner_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameInfo>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub size_bytes: u64,
    pub codec: String,
    pub content_type: String,
    #[serde(default)]
    pub audio_tracks: Vec<AudioTrackInfo>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub visibility: ShareVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub uploaded_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request body for `PATCH /api/v1/clips/{id}` to update clip metadata or visibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpdateClipRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<ShareVisibility>,
}

/// Lifecycle status of a streaming/chunked upload session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UploadSessionStatus {
    #[default]
    Pending,
    Uploading,
    Finalizing,
    Completed,
    Failed,
    Aborted,
}

/// Request body for `POST /api/v1/uploads` to initiate a streaming/chunked upload session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateUploadSessionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_id: Option<Uuid>,
    pub title: String,
    pub filename: String,
    pub total_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default)]
    pub visibility: ShareVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameInfo>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub fps: f32,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub audio_tracks: Vec<AudioTrackInfo>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
}

/// Response returned by `POST /api/v1/uploads`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateUploadSessionResponse {
    pub session_id: String,
    pub clip_id: Uuid,
    pub upload_url: String,
    pub part_size_bytes: u64,
    pub total_bytes: u64,
    pub total_parts: u32,
    pub expires_at: DateTime<Utc>,
}

/// Status of an upload session (`GET /api/v1/uploads/{session_id}` or `POST .../complete`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadStatusDto {
    pub session_id: String,
    pub clip_id: Uuid,
    pub status: UploadSessionStatus,
    pub bytes_received: u64,
    pub total_bytes: u64,
    pub uploaded_parts: Vec<u32>,
    pub total_parts: u32,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<ClipDto>,
}

/// Request body for `POST /api/v1/clips/{id}/shares`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CreateShareRequest {
    #[serde(default)]
    pub visibility: ShareVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_hours: Option<u64>,
}

/// Response returned when creating or updating a share link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateShareResponse {
    pub share_id: String,
    pub clip_id: Uuid,
    pub url: String,
    pub visibility: ShareVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Publicly accessible share metadata (`GET /api/v1/shares/{token}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublicShareDto {
    pub share_id: String,
    pub clip_id: Uuid,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameInfo>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub size_bytes: u64,
    pub codec: String,
    pub content_type: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub visibility: ShareVisibility,
    pub url: String,
    pub video_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub uploaded_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
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

impl From<ClipDto> for RemoteClipSummary {
    fn from(c: ClipDto) -> Self {
        Self {
            share_id: c.share_id.unwrap_or_default(),
            url: c.share_url.unwrap_or_default(),
            uploaded_at: c.uploaded_at,
            expires_at: c.expires_at,
            clip_id: ClipId::from_uuid(c.id),
            title: c.title,
            visibility: c.visibility,
            size_bytes: c.size_bytes,
        }
    }
}

/// `reqwest`-based [`CloudClient`] implementation for ClipForge Server (`/api/v1`).
#[derive(Clone)]
pub struct HttpCloudClient {
    client: reqwest::Client,
    base_url: String,
    token: Option<String>,
}

impl std::fmt::Debug for HttpCloudClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpCloudClient")
            .field("base_url", &self.base_url)
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

fn is_plaintext_non_localhost(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let host_no_port = if host.starts_with('[') {
        host.split(']')
            .next()
            .unwrap_or(host)
            .trim_start_matches('[')
    } else {
        host.split(':').next().unwrap_or(host)
    };
    !matches!(host_no_port, "localhost" | "127.0.0.1" | "::1")
}

impl HttpCloudClient {
    /// Creates a new [`HttpCloudClient`] targeting `base_url` with an optional bearer token or API key.
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        if is_plaintext_non_localhost(&base_url) {
            tracing::warn!(
                base_url = %base_url,
                "HttpCloudClient configured with plaintext http:// URL; bearer tokens will be sent unencrypted"
            );
        }
        Self {
            client: reqwest::Client::new(),
            base_url,
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

    /// Fetches `/api/v1/health` status details.
    pub async fn health_v1(&self) -> Result<HealthDto> {
        let url = format!("{}/api/v1/health", self.base_url);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("health check failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Fetches public server capabilities and limits (`GET /api/v1/server/info`).
    pub async fn server_info(&self) -> Result<ServerInfoDto> {
        let url = format!("{}/api/v1/server/info", self.base_url);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("server info request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Registers a new user account and returns the issued bearer token.
    pub async fn register(&self, username: &str, password: &str) -> Result<AuthResponse> {
        self.register_with_display_name(username, password, None)
            .await
    }

    /// Registers a new user account with an optional display name.
    pub async fn register_with_display_name(
        &self,
        username: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<AuthResponse> {
        let url = format!("{}/api/v1/auth/register", self.base_url);
        let mut body = serde_json::json!({ "username": username, "password": password });
        if let Some(name) = display_name {
            body["display_name"] = serde_json::Value::String(name.to_string());
        }
        let resp = self
            .client
            .post(url)
            .json(&body)
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

    /// Revokes the current session token (`POST /api/v1/auth/logout`).
    pub async fn logout(&self) -> Result<()> {
        let url = format!("{}/api/v1/auth/logout", self.base_url);
        let resp = self
            .apply_auth(self.client.post(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("logout request failed: {e}")))?;

        check_empty_response(resp).await
    }

    /// Fetches the authenticated user's profile and quota (`GET /api/v1/auth/me`).
    pub async fn current_user(&self) -> Result<UserInfo> {
        let url = format!("{}/api/v1/auth/me", self.base_url);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("current user request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Fetches the authenticated user's storage quota (`GET /api/v1/quota`).
    pub async fn quota(&self) -> Result<QuotaDto> {
        let url = format!("{}/api/v1/quota", self.base_url);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("quota request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Lists API tokens for the current user (`GET /api/v1/auth/tokens`).
    pub async fn list_tokens(&self) -> Result<Vec<ApiTokenInfo>> {
        let url = format!("{}/api/v1/auth/tokens", self.base_url);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("list tokens failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Creates a new scoped API token (`POST /api/v1/auth/tokens`).
    pub async fn create_token(&self, req: &CreateTokenRequest) -> Result<CreateTokenResponse> {
        let url = format!("{}/api/v1/auth/tokens", self.base_url);
        let resp = self
            .apply_auth(self.client.post(url))
            .json(req)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("create token failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Revokes an API token by ID (`DELETE /api/v1/auth/tokens/{id}`).
    pub async fn revoke_token(&self, token_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/auth/tokens/{}", self.base_url, token_id);
        let resp = self
            .apply_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("revoke token failed: {e}")))?;

        check_empty_response(resp).await
    }

    /// Initiates a chunked/streaming upload session (`POST /api/v1/uploads`).
    pub async fn create_upload_session(
        &self,
        req: &CreateUploadSessionRequest,
    ) -> Result<CreateUploadSessionResponse> {
        let url = format!("{}/api/v1/uploads", self.base_url);
        let resp = self
            .apply_auth(self.client.post(url))
            .json(req)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("create upload session failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Fetches the current status of an upload session (`GET /api/v1/uploads/{session_id}`).
    pub async fn upload_session_status(&self, session_id: &str) -> Result<UploadStatusDto> {
        let url = format!("{}/api/v1/uploads/{}", self.base_url, session_id);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("upload status request failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Uploads a single 1-indexed part of a chunked upload session.
    pub async fn upload_part(
        &self,
        session_id: &str,
        part_number: u32,
        data: Vec<u8>,
    ) -> Result<UploadStatusDto> {
        let url = format!(
            "{}/api/v1/uploads/{}/parts/{}",
            self.base_url, session_id, part_number
        );
        let resp = self
            .apply_auth(self.client.put(url))
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(data)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("upload part {part_number} failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Finalizes a chunked upload session (`POST /api/v1/uploads/{session_id}/complete`).
    pub async fn complete_upload_session(&self, session_id: &str) -> Result<UploadStatusDto> {
        let url = format!("{}/api/v1/uploads/{}/complete", self.base_url, session_id);
        let resp = self
            .apply_auth(self.client.post(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("complete upload session failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Aborts an in-progress upload session (`DELETE /api/v1/uploads/{session_id}`).
    pub async fn abort_upload_session(&self, session_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/uploads/{}", self.base_url, session_id);
        let resp = self
            .apply_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("abort upload session failed: {e}")))?;

        check_empty_response(resp).await
    }

    /// Uploads a clip using the chunked `/api/v1/uploads` session flow with progress callbacks,
    /// falling back to direct multipart `POST /api/v1/clips` if the server does not support
    /// upload sessions.
    pub async fn upload_clip_streaming<F>(
        &self,
        clip: &ClipMetadata,
        file_path: &Path,
        visibility: ShareVisibility,
        mut on_progress: F,
    ) -> Result<RemoteClipSummary>
    where
        F: FnMut(u8) + Send,
    {
        let mut file = tokio::fs::File::open(file_path).await?;
        let total_bytes = file.metadata().await?.len();
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("clip.mp4")
            .to_string();

        let session_req = CreateUploadSessionRequest {
            clip_id: Some(clip.id.0),
            title: clip.title.clone(),
            filename: file_name,
            total_bytes,
            content_type: Some("video/mp4".to_string()),
            sha256: None,
            visibility,
            game: clip.game.clone(),
            duration_ms: clip.duration_ms,
            width: clip.width,
            height: clip.height,
            fps: clip.fps,
            codec: clip.codec.clone(),
            audio_tracks: clip.audio_tracks.clone(),
            tags: clip.tags.clone(),
            notes: clip.notes.clone(),
            created_at: Some(clip.created_at),
        };

        let session = match self.create_upload_session(&session_req).await {
            Ok(s) => s,
            Err(ClipForgeError::NotFound(_)) => {
                on_progress(10);
                let res = self
                    .upload_clip_with_visibility(clip, file_path, visibility)
                    .await?;
                on_progress(100);
                return Ok(res);
            }
            Err(e) => return Err(e),
        };

        let part_size = session.part_size_bytes.max(64 * 1024) as usize;
        let mut buf = vec![0u8; part_size];
        let mut part_number: u32 = 1;
        let mut sent_bytes: u64 = 0;

        loop {
            let mut filled = 0;
            while filled < part_size {
                let n = file.read(&mut buf[filled..]).await?;
                if n == 0 {
                    break;
                }
                filled += n;
            }
            if filled == 0 {
                if total_bytes == 0
                    && part_number == 1
                    && let Err(err) = self
                        .upload_part(&session.session_id, part_number, Vec::new())
                        .await
                {
                    let _ = self.abort_upload_session(&session.session_id).await;
                    return Err(err);
                }
                break;
            }

            if let Err(err) = self
                .upload_part(&session.session_id, part_number, buf[..filled].to_vec())
                .await
            {
                let _ = self.abort_upload_session(&session.session_id).await;
                return Err(err);
            }

            sent_bytes += filled as u64;
            let pct = (sent_bytes * 95)
                .checked_div(total_bytes)
                .map_or(95, |v| v.min(95)) as u8;
            on_progress(pct);
            part_number += 1;
        }

        let status = match self.complete_upload_session(&session.session_id).await {
            Ok(s) => s,
            Err(err) => {
                let _ = self.abort_upload_session(&session.session_id).await;
                return Err(err);
            }
        };
        on_progress(100);

        if let Some(clip_dto) = status.clip {
            return Ok(clip_dto.into());
        }

        let clip_dto = self.get_clip(&status.clip_id.to_string()).await?;
        Ok(clip_dto.into())
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

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status, &text));
        }
        let raw = resp
            .text()
            .await
            .map_err(|e| ClipForgeError::Serialization(format!("read upload response: {e}")))?;
        if let Ok(summary) = serde_json::from_str::<RemoteClipSummary>(&raw) {
            return Ok(summary);
        }
        serde_json::from_str::<ClipDto>(&raw)
            .map(Into::into)
            .map_err(|e| ClipForgeError::Serialization(format!("decode upload response: {e}")))
    }

    /// Lists clips owned by the authenticated user as full [`ClipDto`] records.
    pub async fn list_clips_v1(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<ClipDto>> {
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

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status, &text));
        }
        let raw = resp
            .text()
            .await
            .map_err(|e| ClipForgeError::Serialization(format!("read clips response: {e}")))?;
        if let Ok(summaries) = serde_json::from_str::<Vec<RemoteClipSummary>>(&raw) {
            return Ok(summaries);
        }
        let dtos: Vec<ClipDto> = serde_json::from_str(&raw)
            .map_err(|e| ClipForgeError::Serialization(format!("decode clips response: {e}")))?;
        Ok(dtos.into_iter().map(Into::into).collect())
    }

    /// Fetches a single clip by ID (`GET /api/v1/clips/{id}`).
    pub async fn get_clip(&self, clip_id: &str) -> Result<ClipDto> {
        let url = format!("{}/api/v1/clips/{}", self.base_url, clip_id);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("get clip failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Updates clip metadata or visibility (`PATCH /api/v1/clips/{id}`).
    pub async fn update_clip(&self, clip_id: &str, req: &UpdateClipRequest) -> Result<ClipDto> {
        let url = format!("{}/api/v1/clips/{}", self.base_url, clip_id);
        let resp = self
            .apply_auth(self.client.patch(url))
            .json(req)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("update clip failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Updates visibility (`public` or `private`) for a clip (`PATCH /api/v1/clips/{clip_id}`).
    pub async fn set_visibility(
        &self,
        clip_id: &str,
        visibility: ShareVisibility,
    ) -> Result<RemoteClipSummary> {
        let clip_url = format!("{}/api/v1/clips/{}", self.base_url, clip_id);
        let resp = self
            .apply_auth(self.client.patch(clip_url))
            .json(&UpdateClipRequest {
                visibility: Some(visibility),
                ..Default::default()
            })
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("set visibility failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status, &text));
        }
        let raw = resp
            .text()
            .await
            .map_err(|e| ClipForgeError::Serialization(format!("read visibility response: {e}")))?;
        if let Ok(dto) = serde_json::from_str::<ClipDto>(&raw) {
            return Ok(dto.into());
        }
        serde_json::from_str::<RemoteClipSummary>(&raw)
            .map_err(|e| ClipForgeError::Serialization(format!("decode visibility response: {e}")))
    }

    /// Creates or updates a share link for a clip (`POST /api/v1/clips/{id}/shares`).
    pub async fn create_share(
        &self,
        clip_id: &str,
        req: &CreateShareRequest,
    ) -> Result<CreateShareResponse> {
        let url = format!("{}/api/v1/clips/{}/shares", self.base_url, clip_id);
        let resp = self
            .apply_auth(self.client.post(url))
            .json(req)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("create share failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Fetches public share metadata (`GET /api/v1/shares/{token}`).
    pub async fn get_public_share(&self, token: &str) -> Result<PublicShareDto> {
        let url = format!("{}/api/v1/shares/{}", self.base_url, token);
        let resp = self
            .apply_auth(self.client.get(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("get public share failed: {e}")))?;

        parse_json_response(resp).await
    }

    /// Revokes a share link by share token/ID (`DELETE /api/v1/shares/{share_id}`).
    pub async fn revoke_share(&self, share_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/shares/{}", self.base_url, share_id);
        let resp = self
            .apply_auth(self.client.delete(url))
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("revoke share failed: {e}")))?;

        check_empty_response(resp).await
    }

    /// Deletes a clip by clip ID (`DELETE /api/v1/clips/{id}`).
    pub async fn delete_clip(&self, clip_id: &str) -> Result<()> {
        let url = format!("{}/api/v1/clips/{}", self.base_url, clip_id);
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
        let url = format!("{}/api/v1/health", self.base_url);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ClipForgeError::Cloud(format!("health check failed: {e}")))?;

        if resp.status().is_success() {
            return Ok(());
        }
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            let fallback = format!("{}/health", self.base_url);
            let fallback_resp = self
                .client
                .get(fallback)
                .send()
                .await
                .map_err(|e| ClipForgeError::Cloud(format!("health check failed: {e}")))?;
            return check_empty_response(fallback_resp).await;
        }
        check_empty_response(resp).await
    }

    async fn upload_clip(&self, clip: &ClipMetadata, file_path: &Path) -> Result<CloudShareInfo> {
        let summary = self
            .upload_clip_streaming(clip, file_path, ShareVisibility::Public, |_| {})
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

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(map_http_error(status, &text));
        }
        let raw = resp
            .text()
            .await
            .map_err(|e| ClipForgeError::Serialization(format!("read share response: {e}")))?;
        if let Ok(info) = serde_json::from_str::<CloudShareInfo>(&raw) {
            return Ok(info);
        }
        let public: PublicShareDto = serde_json::from_str(&raw)
            .map_err(|e| ClipForgeError::Serialization(format!("decode share response: {e}")))?;
        Ok(CloudShareInfo {
            share_id: public.share_id,
            url: public.url,
            uploaded_at: public.uploaded_at,
            expires_at: public.expires_at,
        })
    }

    async fn delete_share(&self, share_id: &str) -> Result<()> {
        self.revoke_share(share_id).await
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
    let message = if let Ok(dto) = serde_json::from_str::<ApiErrorDto>(body) {
        if dto.code.is_empty() {
            dto.error
        } else {
            format!("{} ({})", dto.error, dto.code)
        }
    } else if body.trim().is_empty() {
        status
            .canonical_reason()
            .unwrap_or("request failed")
            .to_string()
    } else {
        body.to_string()
    };

    if status == reqwest::StatusCode::NOT_FOUND {
        ClipForgeError::NotFound(format!("remote resource not found: {message}"))
    } else if status == reqwest::StatusCode::BAD_REQUEST
        || status == reqwest::StatusCode::UNPROCESSABLE_ENTITY
    {
        ClipForgeError::InvalidInput(format!("bad request: {message}"))
    } else {
        ClipForgeError::Cloud(format!("server returned {status}: {message}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_server<F, Fut>(handler: F) -> String
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = (u16, String)> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handler = std::sync::Arc::new(handler);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let h = handler.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 64 * 1024];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let (status, body) = h(req).await;
                    let resp = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

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

    #[test]
    fn map_http_error_parses_api_error_dto() {
        let err_json = serde_json::json!({
            "error": "storage quota exceeded",
            "code": "quota_exceeded",
            "status": 413
        })
        .to_string();
        let mapped = map_http_error(reqwest::StatusCode::PAYLOAD_TOO_LARGE, &err_json);
        let msg = mapped.to_string();
        assert!(msg.contains("storage quota exceeded"));
        assert!(msg.contains("quota_exceeded"));
    }

    #[tokio::test]
    async fn client_fetches_server_info_user_quota_and_streaming_upload() {
        let now = Utc::now();
        let clip_uuid = Uuid::new_v4();
        let base_url = spawn_mock_server(move |req| {
            let now = now;
            async move {
                let first_line = req.lines().next().unwrap_or("");
                if first_line.starts_with("GET /api/v1/server/info ") {
                    let body = serde_json::to_string(&ServerInfoDto {
                        name: "ClipForge".into(),
                        version: "1.2.3".into(),
                        api_version: "v1".into(),
                        storage_backend: "local".into(),
                        max_upload_bytes: 1_000_000_000,
                        chunk_size_bytes: 65_536,
                        registration_enabled: true,
                        public_base_url: None,
                    })
                    .unwrap();
                    (200, body)
                } else if first_line.starts_with("GET /api/v1/auth/me ") {
                    let body = serde_json::to_string(&UserInfo {
                        id: "u1".into(),
                        username: "emkacz".into(),
                        display_name: None,
                        role: UserRole::Admin,
                        is_active: true,
                        created_at: now,
                        quota: Some(QuotaDto {
                            used_bytes: 500,
                            limit_bytes: 2000,
                            remaining_bytes: 1500,
                            clip_count: 1,
                        }),
                    })
                    .unwrap();
                    (200, body)
                } else if first_line.starts_with("POST /api/v1/uploads ") {
                    let body = serde_json::to_string(&CreateUploadSessionResponse {
                        session_id: "sess_1".into(),
                        clip_id: clip_uuid,
                        upload_url: "/api/v1/uploads/sess_1".into(),
                        part_size_bytes: 65_536,
                        total_bytes: 5,
                        total_parts: 1,
                        expires_at: now,
                    })
                    .unwrap();
                    (201, body)
                } else if first_line.starts_with("PUT /api/v1/uploads/sess_1/parts/1 ") {
                    let body = serde_json::to_string(&UploadStatusDto {
                        session_id: "sess_1".into(),
                        clip_id: clip_uuid,
                        status: UploadSessionStatus::Uploading,
                        bytes_received: 5,
                        total_bytes: 5,
                        uploaded_parts: vec![1],
                        total_parts: 1,
                        expires_at: now,
                        clip: None,
                    })
                    .unwrap();
                    (200, body)
                } else if first_line.starts_with("POST /api/v1/uploads/sess_1/complete ") {
                    let dto = ClipDto {
                        id: clip_uuid,
                        owner_id: "u1".into(),
                        title: "Test".into(),
                        game: None,
                        duration_ms: 1000,
                        width: 1920,
                        height: 1080,
                        fps: 60.0,
                        size_bytes: 5,
                        codec: "h264".into(),
                        content_type: "video/mp4".into(),
                        audio_tracks: vec![],
                        tags: vec![],
                        notes: None,
                        visibility: ShareVisibility::Public,
                        share_id: Some("shr_1".into()),
                        share_url: Some("http://localhost/c/shr_1".into()),
                        stream_url: None,
                        thumbnail_url: None,
                        created_at: now,
                        uploaded_at: now,
                        expires_at: None,
                    };
                    let body = serde_json::to_string(&UploadStatusDto {
                        session_id: "sess_1".into(),
                        clip_id: clip_uuid,
                        status: UploadSessionStatus::Completed,
                        bytes_received: 5,
                        total_bytes: 5,
                        uploaded_parts: vec![1],
                        total_parts: 1,
                        expires_at: now,
                        clip: Some(dto),
                    })
                    .unwrap();
                    (200, body)
                } else {
                    (
                        404,
                        r#"{"error":"not found","code":"not_found","status":404}"#.into(),
                    )
                }
            }
        })
        .await;

        let client = HttpCloudClient::new(&base_url, Some("tok_test".into()));
        let info = client.server_info().await.expect("server_info");
        assert_eq!(info.version, "1.2.3");

        let me = client.current_user().await.expect("current_user");
        assert_eq!(me.username, "emkacz");
        assert_eq!(me.quota.unwrap().used_bytes, 500);

        let tmp =
            std::env::temp_dir().join(format!("clipforge-upload-test-{}.mp4", Uuid::new_v4()));
        std::fs::write(&tmp, b"hello").unwrap();
        let meta = ClipMetadata {
            id: ClipId::from_uuid(clip_uuid),
            title: "Test".into(),
            game: None,
            file_path: tmp.clone(),
            thumbnail_path: None,
            created_at: now,
            duration_ms: 1000,
            width: 1920,
            height: 1080,
            fps: 60.0,
            size_bytes: 5,
            codec: "h264".into(),
            audio_tracks: vec![],
            tags: vec![],
            cloud_share: None,
            notes: None,
        };
        let mut pcts = Vec::new();
        let summary = client
            .upload_clip_streaming(&meta, &tmp, ShareVisibility::Public, |p| pcts.push(p))
            .await
            .expect("streaming upload");
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(summary.share_id, "shr_1");
        assert_eq!(pcts.last(), Some(&100));
    }

    #[tokio::test]
    async fn set_visibility_patches_clip_id_and_surfaces_http_errors() {
        let now = Utc::now();
        let clip_uuid = Uuid::new_v4();
        let err_uuid = Uuid::new_v4();
        let patch_path = format!("PATCH /api/v1/clips/{clip_uuid} ");
        let err_path = format!("PATCH /api/v1/clips/{err_uuid} ");

        let base_url = spawn_mock_server(move |req| {
            let patch_path = patch_path.clone();
            let err_path = err_path.clone();
            async move {
                let first_line = req.lines().next().unwrap_or("");
                if first_line.starts_with(&patch_path) {
                    assert!(req.contains(r#""visibility":"private""#));
                    let dto = ClipDto {
                        id: clip_uuid,
                        owner_id: "u1".into(),
                        title: "Clip".into(),
                        game: None,
                        duration_ms: 1000,
                        width: 1920,
                        height: 1080,
                        fps: 60.0,
                        size_bytes: 123,
                        codec: "h264".into(),
                        content_type: "video/mp4".into(),
                        audio_tracks: vec![],
                        tags: vec![],
                        notes: None,
                        visibility: ShareVisibility::Private,
                        share_id: Some("shr_tok".into()),
                        share_url: Some("http://localhost/c/shr_tok".into()),
                        stream_url: None,
                        thumbnail_url: None,
                        created_at: now,
                        uploaded_at: now,
                        expires_at: None,
                    };
                    (200, serde_json::to_string(&dto).unwrap())
                } else if first_line.starts_with(&err_path) {
                    (
                        403,
                        r#"{"error":"forbidden","code":"forbidden","status":403}"#.into(),
                    )
                } else {
                    (
                        404,
                        r#"{"error":"not found","code":"not_found","status":404}"#.into(),
                    )
                }
            }
        })
        .await;

        let client = HttpCloudClient::new(&base_url, Some("tok_test".into()));
        let summary = client
            .set_visibility(&clip_uuid.to_string(), ShareVisibility::Private)
            .await
            .expect("set_visibility should succeed");
        assert_eq!(summary.clip_id, ClipId::from_uuid(clip_uuid));
        assert_eq!(summary.share_id, "shr_tok");
        assert_eq!(summary.visibility, ShareVisibility::Private);

        let err = client
            .set_visibility(&err_uuid.to_string(), ShareVisibility::Public)
            .await
            .expect_err("403 must not be masked by legacy fallback");
        assert!(err.to_string().contains("forbidden"));
    }

    #[tokio::test]
    async fn delete_clip_targets_clip_endpoint_and_list_clips_maps_unshared_clip() {
        let now = Utc::now();
        let clip_uuid = Uuid::new_v4();
        let fail_uuid = Uuid::new_v4();
        let del_ok = format!("DELETE /api/v1/clips/{clip_uuid} ");
        let del_fail = format!("DELETE /api/v1/clips/{fail_uuid} ");

        let base_url = spawn_mock_server(move |req| {
            let del_ok = del_ok.clone();
            let del_fail = del_fail.clone();
            async move {
                let first_line = req.lines().next().unwrap_or("");
                if first_line.starts_with(&del_ok) {
                    (204, String::new())
                } else if first_line.starts_with(&del_fail) {
                    (
                        502,
                        r#"{"error":"s3 delete failed","code":"storage_error","status":502}"#
                            .into(),
                    )
                } else if first_line.starts_with("GET /api/v1/clips ") {
                    let dto = ClipDto {
                        id: clip_uuid,
                        owner_id: "u1".into(),
                        title: "Unshared Clip".into(),
                        game: None,
                        duration_ms: 2000,
                        width: 1920,
                        height: 1080,
                        fps: 60.0,
                        size_bytes: 456,
                        codec: "h264".into(),
                        content_type: "video/mp4".into(),
                        audio_tracks: vec![],
                        tags: vec![],
                        notes: None,
                        visibility: ShareVisibility::Private,
                        share_id: None,
                        share_url: None,
                        stream_url: None,
                        thumbnail_url: None,
                        created_at: now,
                        uploaded_at: now,
                        expires_at: None,
                    };
                    (200, serde_json::to_string(&vec![dto]).unwrap())
                } else {
                    (
                        404,
                        r#"{"error":"not found","code":"not_found","status":404}"#.into(),
                    )
                }
            }
        })
        .await;

        let client = HttpCloudClient::new(&base_url, Some("tok_test".into()));
        let clips = client.list_clips(None, None).await.expect("list_clips");
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].clip_id, ClipId::from_uuid(clip_uuid));
        assert!(clips[0].share_id.is_empty());
        assert_eq!(clips[0].visibility, ShareVisibility::Private);

        client
            .delete_clip(&clip_uuid.to_string())
            .await
            .expect("delete_clip 204");

        let err = client
            .delete_clip(&fail_uuid.to_string())
            .await
            .expect_err("502 must surface error");
        assert!(err.to_string().contains("s3 delete failed"));
    }
}
