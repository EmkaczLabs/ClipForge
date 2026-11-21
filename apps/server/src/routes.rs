use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRequestParts, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Utc};
use clipforge_common::{ClipId, ClipMetadata, CloudShareInfo};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::auth::{extract_raw_credential, generate_secret_token, hash_password, verify_password};
use crate::config::ServerConfig;
use crate::db::{ApiKeySummary, AuthUser, Database, NewClipParams, StoredClip, Visibility};
use crate::error::ServerError;
use crate::storage::{ObjectStorage, content_type_for_ext, sanitize_video_extension};

const EMBEDDED_PLAYER_HTML: &str = include_str!("../../web/index.html");

/// Shared application state passed to Axum handlers.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ServerConfig>,
    pub db: Database,
    pub storage: Arc<dyn ObjectStorage>,
}

/// Builds the complete Axum [`Router`] for `clipforge-server`.
pub fn build_router(state: AppState) -> Router {
    let upload_route = Router::new()
        .route("/api/v1/clips", post(upload_clip))
        .layer(DefaultBodyLimit::disable());

    Router::new()
        .route("/health", get(health))
        .route("/s/{token}", get(serve_player_page))
        // Auth endpoints
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route(
            "/api/v1/auth/api-keys",
            get(list_api_keys).post(create_api_key),
        )
        .route("/api/v1/auth/api-keys/{id}", delete(revoke_api_key))
        // Clip management endpoints
        .merge(upload_route)
        .route("/api/v1/clips", get(list_clips))
        .route(
            "/api/v1/clips/{id}",
            get(get_share_info)
                .patch(set_visibility)
                .delete(delete_clip),
        )
        .route("/api/v1/clips/{id}/visibility", patch(set_visibility))
        // Share endpoints
        .route(
            "/api/v1/shares/{token}",
            get(get_share_info).delete(delete_clip),
        )
        .route("/api/v1/shares/{token}/visibility", patch(set_visibility))
        .route("/api/v1/shares/{token}/meta", get(get_share_meta))
        .route(
            "/api/v1/shares/{token}/video",
            get(stream_share_video).head(head_share_video),
        )
        .with_state(state)
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ServerError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let api_key_header = parts.headers.get("x-api-key").and_then(|v| v.to_str().ok());
        let query_token = parts.uri.query().and_then(extract_query_token);

        let raw = extract_raw_credential(auth_header, api_key_header, query_token.as_deref())
            .ok_or_else(|| {
                ServerError::Unauthorized("missing authentication credentials".to_string())
            })?;

        state
            .db
            .authenticate_credential(&raw)
            .await?
            .ok_or_else(|| ServerError::Unauthorized("invalid or expired credentials".to_string()))
    }
}

fn extract_query_token(query: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=')
            && k == "token"
            && !v.is_empty()
        {
            return Some(v.to_string());
        }
    }
    None
}

async fn try_authenticate(
    headers: &HeaderMap,
    query_token: Option<&str>,
    state: &AppState,
) -> Result<Option<AuthUser>, ServerError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let api_key_header = headers.get("x-api-key").and_then(|v| v.to_str().ok());

    let Some(raw) = extract_raw_credential(auth_header, api_key_header, query_token) else {
        return Ok(None);
    };
    state.db.authenticate_credential(&raw).await
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn serve_player_page(State(state): State<AppState>) -> Html<String> {
    if let Some(web_dir) = &state.config.web_dir {
        let path = web_dir.join("index.html");
        if let Ok(contents) = tokio::fs::read_to_string(&path).await {
            return Html(contents);
        }
    }
    Html(EMBEDDED_PLAYER_HTML.to_string())
}

// ---------------------------------------------------------------------------
// Auth DTOs & Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    user_id: String,
    username: String,
    token: String,
    expires_at: DateTime<Utc>,
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), ServerError> {
    let username = req.username.trim().to_string();
    if username.len() < 3 || username.len() > 64 {
        return Err(ServerError::BadRequest(
            "username must be between 3 and 64 characters".to_string(),
        ));
    }

    let password_hash = hash_password(&req.password)?;
    let user_id = state
        .db
        .create_user(username.clone(), password_hash)
        .await?;
    let token = generate_secret_token("cf_tok_")?;
    let expires_at = state
        .db
        .create_session(user_id.clone(), &token, state.config.session_ttl_hours)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user_id,
            username,
            token,
            expires_at,
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ServerError> {
    let username = req.username.trim().to_string();
    let Some((user_id, password_hash)) = state.db.get_user_by_username(username.clone()).await?
    else {
        return Err(ServerError::Unauthorized(
            "invalid username or password".to_string(),
        ));
    };

    if !verify_password(&req.password, &password_hash) {
        return Err(ServerError::Unauthorized(
            "invalid username or password".to_string(),
        ));
    }

    let token = generate_secret_token("cf_tok_")?;
    let expires_at = state
        .db
        .create_session(user_id.clone(), &token, state.config.session_ttl_hours)
        .await?;

    Ok(Json(AuthResponse {
        user_id,
        username,
        token,
        expires_at,
    }))
}

async fn logout(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, ServerError> {
    state.db.revoke_session(user.token_hash).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateApiKeyResponse {
    id: String,
    name: String,
    key_prefix: String,
    key: String,
    created_at: DateTime<Utc>,
}

async fn list_api_keys(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ApiKeySummary>>, ServerError> {
    let keys = state.db.list_api_keys(user.user_id).await?;
    Ok(Json(keys))
}

async fn create_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), ServerError> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(ServerError::BadRequest(
            "api key name must be 1..=64 characters".to_string(),
        ));
    }
    let raw_key = generate_secret_token("cf_key_")?;
    let summary = state
        .db
        .create_api_key(user.user_id, name.clone(), &raw_key)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            id: summary.id,
            name: summary.name,
            key_prefix: summary.key_prefix,
            key: raw_key,
            created_at: summary.created_at,
        }),
    ))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ServerError> {
    let revoked = state.db.revoke_api_key(user.user_id, id).await?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ServerError::NotFound("api key not found".to_string()))
    }
}

// ---------------------------------------------------------------------------
// Clip Upload & Management Handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ShareResponse {
    share_id: String,
    url: String,
    uploaded_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    clip_id: ClipId,
    title: String,
    visibility: Visibility,
    size_bytes: u64,
}

fn build_share_url(config: &ServerConfig, headers: &HeaderMap, share_id: &str) -> String {
    if let Some(base) = &config.public_base_url {
        return format!("{base}/s/{share_id}");
    }
    if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        return format!("http://{host}/s/{share_id}");
    }
    format!("/s/{share_id}")
}

fn to_share_response(
    clip: &StoredClip,
    config: &ServerConfig,
    headers: &HeaderMap,
) -> ShareResponse {
    ShareResponse {
        share_id: clip.share_id.clone(),
        url: build_share_url(config, headers, &clip.share_id),
        uploaded_at: clip.uploaded_at,
        expires_at: clip.expires_at,
        clip_id: clip.clip_id,
        title: clip.title.clone(),
        visibility: clip.visibility,
        size_bytes: clip.size_bytes,
    }
}

async fn upload_clip(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ShareResponse>), ServerError> {
    let mut metadata_opt: Option<ClipMetadata> = None;
    let mut visibility_opt: Option<Visibility> = None;
    let mut temp_file: Option<tempfile::NamedTempFile> = None;
    let mut uploaded_size: u64 = 0;
    let mut original_filename: Option<String> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| ServerError::BadRequest(format!("invalid multipart payload: {e}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        match field_name.as_str() {
            "metadata" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ServerError::BadRequest(format!("invalid metadata field: {e}")))?;
                let meta: ClipMetadata = serde_json::from_str(&text)
                    .map_err(|e| ServerError::BadRequest(format!("invalid metadata json: {e}")))?;
                metadata_opt = Some(meta);
            }
            "visibility" => {
                let text = field.text().await.map_err(|e| {
                    ServerError::BadRequest(format!("invalid visibility field: {e}"))
                })?;
                visibility_opt = Some(Visibility::parse(text.trim())?);
            }
            "file" => {
                original_filename = field.file_name().map(ToString::to_string);
                let named = tempfile::NamedTempFile::new()?;
                let std_file = named.reopen()?;
                let mut async_file = tokio::fs::File::from_std(std_file);
                let mut total: u64 = 0;

                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| ServerError::BadRequest(format!("upload stream error: {e}")))?
                {
                    total = total.saturating_add(chunk.len() as u64);
                    if total > state.config.max_upload_bytes {
                        return Err(ServerError::PayloadTooLarge(format!(
                            "upload exceeds max size of {} bytes",
                            state.config.max_upload_bytes
                        )));
                    }
                    async_file.write_all(&chunk).await?;
                }
                async_file.flush().await?;
                uploaded_size = total;
                temp_file = Some(named);
            }
            _ => {}
        }
    }

    let named_temp = temp_file
        .ok_or_else(|| ServerError::BadRequest("missing 'file' multipart field".to_string()))?;
    if uploaded_size == 0 {
        return Err(ServerError::BadRequest(
            "uploaded file is empty".to_string(),
        ));
    }

    let meta = metadata_opt
        .ok_or_else(|| ServerError::BadRequest("missing 'metadata' multipart field".to_string()))?;

    let ext = sanitize_video_extension(
        original_filename
            .as_deref()
            .or_else(|| meta.file_path.to_str()),
    );
    let content_type = content_type_for_ext(ext).to_string();
    let share_id = generate_share_id();
    let storage_key = format!("{}/{}.{}", user.user_id, meta.id, ext);

    state
        .storage
        .put_file(
            &storage_key,
            named_temp.path(),
            uploaded_size,
            &content_type,
        )
        .await?;

    let stored = state
        .db
        .insert_clip(NewClipParams {
            clip_id: meta.id,
            owner_id: user.user_id,
            share_id,
            title: meta.title,
            game: meta.game,
            duration_ms: meta.duration_ms,
            width: meta.width,
            height: meta.height,
            fps: meta.fps,
            size_bytes: uploaded_size,
            codec: meta.codec,
            audio_tracks: meta.audio_tracks,
            tags: meta.tags,
            notes: meta.notes,
            storage_key,
            content_type,
            visibility: visibility_opt.unwrap_or(Visibility::Public),
            created_at: meta.created_at,
            expires_at: None,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(to_share_response(&stored, &state.config, &headers)),
    ))
}

fn generate_share_id() -> String {
    Uuid::new_v4().simple().to_string()[..12].to_string()
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_clips(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ShareResponse>>, ServerError> {
    let clips = state
        .db
        .list_clips(user.user_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;
    let resp = clips
        .iter()
        .map(|c| to_share_response(c, &state.config, &headers))
        .collect();
    Ok(Json(resp))
}

async fn authorize_share_read(
    clip: &StoredClip,
    headers: &HeaderMap,
    query_token: Option<&str>,
    state: &AppState,
) -> Result<(), ServerError> {
    if clip.visibility == Visibility::Public {
        return Ok(());
    }
    let Some(user) = try_authenticate(headers, query_token, state).await? else {
        return Err(ServerError::Unauthorized(
            "authentication required for private clip".to_string(),
        ));
    };
    if user.user_id != clip.owner_id {
        return Err(ServerError::Forbidden(
            "you do not have access to this private clip".to_string(),
        ));
    }
    Ok(())
}

async fn get_share_info(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<CloudShareInfo>, ServerError> {
    let clip = state
        .db
        .get_by_share_or_clip_id(id)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    authorize_share_read(&clip, &headers, None, &state).await?;

    Ok(Json(CloudShareInfo {
        share_id: clip.share_id.clone(),
        url: build_share_url(&state.config, &headers, &clip.share_id),
        uploaded_at: clip.uploaded_at,
        expires_at: clip.expires_at,
    }))
}

#[derive(Debug, Serialize)]
struct ShareMetaResponse {
    share_id: String,
    clip_id: ClipId,
    title: String,
    game: Option<clipforge_common::GameInfo>,
    duration_ms: u64,
    width: u32,
    height: u32,
    fps: f32,
    size_bytes: u64,
    codec: String,
    tags: Vec<String>,
    notes: Option<String>,
    visibility: Visibility,
    created_at: DateTime<Utc>,
    uploaded_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    url: String,
}

async fn get_share_meta(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    Path(token): Path<String>,
) -> Result<Json<ShareMetaResponse>, ServerError> {
    let clip = state
        .db
        .get_by_share_or_clip_id(token)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    let query_tok = params.get("token").map(String::as_str);
    authorize_share_read(&clip, &headers, query_tok, &state).await?;

    Ok(Json(ShareMetaResponse {
        share_id: clip.share_id.clone(),
        clip_id: clip.clip_id,
        title: clip.title,
        game: clip.game,
        duration_ms: clip.duration_ms,
        width: clip.width,
        height: clip.height,
        fps: clip.fps,
        size_bytes: clip.size_bytes,
        codec: clip.codec,
        tags: clip.tags,
        notes: clip.notes,
        visibility: clip.visibility,
        created_at: clip.created_at,
        uploaded_at: clip.uploaded_at,
        expires_at: clip.expires_at,
        url: build_share_url(&state.config, &headers, &clip.share_id),
    }))
}

#[derive(Debug, Deserialize)]
struct SetVisibilityRequest {
    visibility: Visibility,
}

async fn set_visibility(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<SetVisibilityRequest>,
) -> Result<Json<ShareResponse>, ServerError> {
    let existing = state
        .db
        .get_by_share_or_clip_id(id.clone())
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    if existing.owner_id != user.user_id {
        return Err(ServerError::Forbidden(
            "you do not own this clip".to_string(),
        ));
    }

    let updated = state
        .db
        .set_visibility(id, user.user_id, req.visibility)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    Ok(Json(to_share_response(&updated, &state.config, &headers)))
}

async fn delete_clip(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, ServerError> {
    let deleted = state
        .db
        .delete_clip(id, user.user_id)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    state.storage.delete(&deleted.storage_key).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Byte-Range Video Streaming Handlers
// ---------------------------------------------------------------------------

async fn head_share_video(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    Path(token): Path<String>,
) -> Result<Response, ServerError> {
    let clip = state
        .db
        .get_by_share_or_clip_id(token)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    let query_tok = params.get("token").map(String::as_str);
    authorize_share_read(&clip, &headers, query_tok, &state).await?;

    let meta = state.storage.head(&clip.storage_key).await?;
    let total = meta.size_bytes;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, clip.content_type),
            (header::CONTENT_LENGTH, total.to_string()),
            (header::ACCEPT_RANGES, "bytes".to_string()),
        ],
    )
        .into_response())
}

async fn stream_share_video(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
    Path(token): Path<String>,
) -> Result<Response, ServerError> {
    let clip = state
        .db
        .get_by_share_or_clip_id(token)
        .await?
        .ok_or_else(|| ServerError::NotFound("clip not found".to_string()))?;

    let query_tok = params.get("token").map(String::as_str);
    authorize_share_read(&clip, &headers, query_tok, &state).await?;

    let meta = state.storage.head(&clip.storage_key).await?;
    let total = meta.size_bytes;
    if total == 0 {
        return Err(ServerError::NotFound("empty video object".to_string()));
    }

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    if let Some(range_str) = range_header {
        let (start, end) = parse_byte_range(range_str, total)
            .ok_or(ServerError::RangeNotSatisfiable { total_bytes: total })?;
        let chunk_len = end - start + 1;
        let stream = state
            .storage
            .get_range(&clip.storage_key, start, end)
            .await?;

        Ok((
            StatusCode::PARTIAL_CONTENT,
            [
                (header::CONTENT_TYPE, clip.content_type),
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_LENGTH, chunk_len.to_string()),
                (
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                ),
            ],
            Body::from_stream(stream),
        )
            .into_response())
    } else {
        let stream = state
            .storage
            .get_range(&clip.storage_key, 0, total - 1)
            .await?;

        Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, clip.content_type),
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_LENGTH, total.to_string()),
            ],
            Body::from_stream(stream),
        )
            .into_response())
    }
}

/// Parses an HTTP `Range: bytes=...` header into an inclusive `(start, end)` pair.
pub fn parse_byte_range(header_val: &str, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }
    let spec = header_val.trim().strip_prefix("bytes=")?;
    // Handle single range (multi-range not needed for HTML5 video seeking)
    let first = spec.split(',').next()?.trim();
    let (start_s, end_s) = first.split_once('-')?;

    if start_s.is_empty() {
        // Suffix range: bytes=-N
        let suffix: u64 = end_s.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = total.saturating_sub(suffix);
        Some((start, total - 1))
    } else {
        let start: u64 = start_s.parse().ok()?;
        if start >= total {
            return None;
        }
        let end = if end_s.is_empty() {
            total - 1
        } else {
            let parsed_end: u64 = end_s.parse().ok()?;
            if parsed_end < start {
                return None;
            }
            parsed_end.min(total - 1)
        };
        Some((start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_byte_range;

    #[test]
    fn byte_range_parsing() {
        assert_eq!(parse_byte_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_byte_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_byte_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_byte_range("bytes=0-5000", 1000), Some((0, 999)));
        assert_eq!(parse_byte_range("bytes=1000-1005", 1000), None);
        assert_eq!(parse_byte_range("bytes=200-100", 1000), None);
    }
}
