use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use clipforge_cloud_client::{
    ApiErrorDto, ApiTokenInfo, AuthResponse, ClipDto, ClipMetadata, CloudClient,
    CreateShareRequest, CreateShareResponse, CreateTokenRequest, CreateTokenResponse,
    CreateUploadSessionRequest, CreateUploadSessionResponse, HealthDto, HttpCloudClient,
    PublicShareDto, QuotaDto, ServerInfoDto, ShareVisibility, UpdateClipRequest,
    UploadSessionStatus, UploadStatusDto, UserInfo, UserRole,
};
use clipforge_common::{ClipForgeError, ClipId, GameInfo};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<MockServerState>>,
}

struct MockUser {
    id: String,
    username: String,
    display_name: Option<String>,
    password: String,
}

struct MockToken {
    info: ApiTokenInfo,
}

struct MockUploadSession {
    session_id: String,
    clip_id: Uuid,
    title: String,
    visibility: ShareVisibility,
    total_bytes: u64,
    total_parts: u32,
    parts: BTreeMap<u32, Vec<u8>>,
    status: UploadSessionStatus,
    clip: Option<ClipDto>,
}

struct MockServerState {
    health_v1_404: bool,
    health_unhealthy: bool,
    uploads_404: bool,
    max_upload_bytes: u64,
    users_by_name: HashMap<String, MockUser>,
    active_sessions: HashMap<String, String>, // bearer token -> username
    api_tokens: HashMap<String, MockToken>,   // token id -> MockToken
    upload_sessions: HashMap<String, MockUploadSession>,
    clips: HashMap<Uuid, ClipDto>,
    shares: HashMap<String, Uuid>, // share_id -> clip_id
    uploaded_payloads: HashMap<Uuid, Vec<u8>>,
    last_multipart_body: Option<Vec<u8>>,
}

impl Default for MockServerState {
    fn default() -> Self {
        Self {
            health_v1_404: false,
            health_unhealthy: false,
            uploads_404: false,
            max_upload_bytes: 10 * 1024 * 1024,
            users_by_name: HashMap::new(),
            active_sessions: HashMap::new(),
            api_tokens: HashMap::new(),
            upload_sessions: HashMap::new(),
            clips: HashMap::new(),
            shares: HashMap::new(),
            uploaded_payloads: HashMap::new(),
            last_multipart_body: None,
        }
    }
}

type ApiResult<T> = std::result::Result<(StatusCode, Json<T>), (StatusCode, Json<ApiErrorDto>)>;

fn api_err(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<ApiErrorDto>) {
    (
        status,
        Json(ApiErrorDto {
            error: message.into(),
            code: code.to_string(),
            status: status.as_u16(),
            details: None,
        }),
    )
}

fn require_bearer(
    headers: &HeaderMap,
    state: &MockServerState,
) -> std::result::Result<String, (StatusCode, Json<ApiErrorDto>)> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| {
            api_err(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing bearer token",
            )
        })?;

    state.active_sessions.get(auth).cloned().ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_token",
            "invalid or expired token",
        )
    })
}

fn compute_quota(state: &MockServerState) -> QuotaDto {
    let used_bytes: u64 = state.clips.values().map(|c| c.size_bytes).sum();
    let limit_bytes = 100 * 1024 * 1024;
    QuotaDto {
        used_bytes,
        limit_bytes,
        remaining_bytes: limit_bytes.saturating_sub(used_bytes),
        clip_count: state.clips.len() as u64,
    }
}

async fn spawn_contract_server() -> (
    String,
    Arc<Mutex<MockServerState>>,
    tokio::task::JoinHandle<()>,
) {
    let inner = Arc::new(Mutex::new(MockServerState::default()));
    let state = AppState {
        inner: inner.clone(),
    };

    let app = Router::new()
        .route("/health", get(health_fallback))
        .route("/api/v1/health", get(health_v1))
        .route("/api/v1/server/info", get(server_info))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/me", get(auth_me))
        .route("/api/v1/quota", get(get_quota))
        .route("/api/v1/auth/tokens", get(list_tokens).post(create_token))
        .route("/api/v1/auth/tokens/{id}", delete(revoke_token))
        .route("/api/v1/uploads", post(create_upload_session))
        .route(
            "/api/v1/uploads/{id}",
            get(get_upload_status).delete(abort_upload_session),
        )
        .route("/api/v1/uploads/{id}/parts/{n}", put(upload_part))
        .route("/api/v1/uploads/{id}/complete", post(complete_upload))
        .route("/api/v1/clips", get(list_clips).post(upload_clip_multipart))
        .route(
            "/api/v1/clips/{id}",
            get(get_clip).patch(update_clip).delete(delete_clip),
        )
        .route("/api/v1/clips/{id}/shares", post(create_share))
        .route(
            "/api/v1/shares/{token}",
            get(get_public_share).delete(revoke_share),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    (format!("http://{addr}"), inner, handle)
}

async fn health_v1(State(state): State<AppState>) -> ApiResult<HealthDto> {
    let guard = state.inner.lock().await;
    if guard.health_v1_404 {
        return Err(api_err(StatusCode::NOT_FOUND, "not_found", "not found"));
    }
    if guard.health_unhealthy {
        return Err(api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "unhealthy",
            "database unavailable",
        ));
    }
    Ok((
        StatusCode::OK,
        Json(HealthDto {
            status: "ok".to_string(),
            version: "0.2.0".to_string(),
            db_ok: true,
            storage_ok: true,
            uptime_secs: 42,
        }),
    ))
}

async fn health_fallback(State(state): State<AppState>) -> ApiResult<HealthDto> {
    let guard = state.inner.lock().await;
    if guard.health_unhealthy {
        return Err(api_err(
            StatusCode::SERVICE_UNAVAILABLE,
            "unhealthy",
            "database unavailable",
        ));
    }
    Ok((
        StatusCode::OK,
        Json(HealthDto {
            status: "ok".to_string(),
            version: "0.2.0".to_string(),
            db_ok: true,
            storage_ok: true,
            uptime_secs: 42,
        }),
    ))
}

async fn server_info(State(state): State<AppState>) -> Json<ServerInfoDto> {
    let guard = state.inner.lock().await;
    Json(ServerInfoDto {
        name: "ClipForge Server".to_string(),
        version: "0.2.0".to_string(),
        api_version: "v1".to_string(),
        storage_backend: "local".to_string(),
        max_upload_bytes: guard.max_upload_bytes,
        chunk_size_bytes: 65_536,
        registration_enabled: true,
        public_base_url: Some("https://clips.example.test".to_string()),
    })
}

#[derive(Deserialize)]
struct RegisterPayload {
    username: String,
    password: String,
    display_name: Option<String>,
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterPayload>,
) -> ApiResult<AuthResponse> {
    let mut guard = state.inner.lock().await;
    if guard.users_by_name.contains_key(&req.username) {
        return Err(api_err(
            StatusCode::CONFLICT,
            "username_taken",
            "username already exists",
        ));
    }
    let user_id = format!("usr_{}", Uuid::new_v4().simple());
    let token = format!("cf_sess_{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let user_info = UserInfo {
        id: user_id.clone(),
        username: req.username.clone(),
        display_name: req.display_name.clone(),
        role: UserRole::User,
        is_active: true,
        created_at: now,
        quota: Some(compute_quota(&guard)),
    };
    guard.users_by_name.insert(
        req.username.clone(),
        MockUser {
            id: user_id.clone(),
            username: req.username.clone(),
            display_name: req.display_name,
            password: req.password,
        },
    );
    guard
        .active_sessions
        .insert(token.clone(), req.username.clone());

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user_id,
            username: req.username,
            token,
            expires_at: now + Duration::hours(24),
            user: Some(user_info),
        }),
    ))
}

#[derive(Deserialize)]
struct LoginPayload {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginPayload>,
) -> ApiResult<AuthResponse> {
    let mut guard = state.inner.lock().await;
    let user = guard.users_by_name.get(&req.username).ok_or_else(|| {
        api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "invalid username or password",
        )
    })?;
    if user.password != req.password {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "invalid username or password",
        ));
    }
    let user_id = user.id.clone();
    let display_name = user.display_name.clone();
    let token = format!("cf_sess_{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let quota = compute_quota(&guard);
    guard
        .active_sessions
        .insert(token.clone(), req.username.clone());

    Ok((
        StatusCode::OK,
        Json(AuthResponse {
            user_id: user_id.clone(),
            username: req.username.clone(),
            token,
            expires_at: now + Duration::hours(24),
            user: Some(UserInfo {
                id: user_id,
                username: req.username,
                display_name,
                role: UserRole::User,
                is_active: true,
                created_at: now,
                quota: Some(quota),
            }),
        }),
    ))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> std::result::Result<StatusCode, (StatusCode, Json<ApiErrorDto>)> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    guard.active_sessions.remove(&token);
    Ok(StatusCode::NO_CONTENT)
}

async fn auth_me(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<UserInfo> {
    let guard = state.inner.lock().await;
    let username = require_bearer(&headers, &guard)?;
    let user = guard
        .users_by_name
        .get(&username)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "user not found"))?;
    Ok((
        StatusCode::OK,
        Json(UserInfo {
            id: user.id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: UserRole::User,
            is_active: true,
            created_at: Utc::now(),
            quota: Some(compute_quota(&guard)),
        }),
    ))
}

async fn get_quota(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<QuotaDto> {
    let guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    Ok((StatusCode::OK, Json(compute_quota(&guard))))
}

async fn list_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Vec<ApiTokenInfo>> {
    let guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let tokens = guard.api_tokens.values().map(|t| t.info.clone()).collect();
    Ok((StatusCode::OK, Json(tokens)))
}

async fn create_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateTokenRequest>,
) -> ApiResult<CreateTokenResponse> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    if req.name.trim().is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_token_name",
            "token name cannot be empty",
        ));
    }
    let id = format!("tok_{}", Uuid::new_v4().simple());
    let raw_token = format!("cf_tok_{}", Uuid::new_v4().simple());
    let prefix = raw_token[..12].to_string();
    let now = Utc::now();
    let expires_at = req
        .expires_in_days
        .map(|days| now + Duration::days(i64::from(days)));

    let info = ApiTokenInfo {
        id: id.clone(),
        name: req.name.clone(),
        prefix: prefix.clone(),
        scopes: req.scopes.clone(),
        created_at: now,
        expires_at,
        last_used_at: None,
    };
    guard.api_tokens.insert(id.clone(), MockToken { info });

    Ok((
        StatusCode::CREATED,
        Json(CreateTokenResponse {
            id,
            name: req.name,
            prefix,
            token: raw_token,
            scopes: req.scopes,
            created_at: now,
            expires_at,
        }),
    ))
}

async fn revoke_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> std::result::Result<StatusCode, (StatusCode, Json<ApiErrorDto>)> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    if guard.api_tokens.remove(&id).is_none() {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "token not found",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn create_upload_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateUploadSessionRequest>,
) -> ApiResult<CreateUploadSessionResponse> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;

    if guard.uploads_404 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "chunked uploads not enabled",
        ));
    }
    if req.total_bytes > guard.max_upload_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiErrorDto {
                error: format!(
                    "upload size {} exceeds max_upload_bytes {}",
                    req.total_bytes, guard.max_upload_bytes
                ),
                code: "payload_too_large".to_string(),
                status: 413,
                details: Some(serde_json::json!({
                    "total_bytes": req.total_bytes,
                    "max_upload_bytes": guard.max_upload_bytes
                })),
            }),
        ));
    }

    let part_size_bytes: u64 = 65_536;
    let total_parts = req.total_bytes.div_ceil(part_size_bytes).max(1) as u32;
    let session_id = format!("sess_{}", Uuid::new_v4().simple());
    let clip_id = req.clip_id.unwrap_or_else(Uuid::new_v4);
    let expires_at = Utc::now() + Duration::hours(1);

    guard.upload_sessions.insert(
        session_id.clone(),
        MockUploadSession {
            session_id: session_id.clone(),
            clip_id,
            title: req.title,
            visibility: req.visibility,
            total_bytes: req.total_bytes,
            total_parts,
            parts: BTreeMap::new(),
            status: UploadSessionStatus::Pending,
            clip: None,
        },
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateUploadSessionResponse {
            session_id: session_id.clone(),
            clip_id,
            upload_url: format!("/api/v1/uploads/{session_id}"),
            part_size_bytes,
            total_bytes: req.total_bytes,
            total_parts,
            expires_at,
        }),
    ))
}

async fn get_upload_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<UploadStatusDto> {
    let guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let sess = guard.upload_sessions.get(&id).ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "upload session not found",
        )
    })?;

    let bytes_received: u64 = sess.parts.values().map(|p| p.len() as u64).sum();
    let uploaded_parts: Vec<u32> = sess.parts.keys().copied().collect();
    Ok((
        StatusCode::OK,
        Json(UploadStatusDto {
            session_id: sess.session_id.clone(),
            clip_id: sess.clip_id,
            status: sess.status,
            bytes_received,
            total_bytes: sess.total_bytes,
            uploaded_parts,
            total_parts: sess.total_parts,
            expires_at: Utc::now() + Duration::hours(1),
            clip: sess.clip.clone(),
        }),
    ))
}

async fn upload_part(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((id, n)): AxumPath<(String, u32)>,
    body: Bytes,
) -> ApiResult<UploadStatusDto> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if content_type != "application/octet-stream" {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_content_type",
            "expected application/octet-stream",
        ));
    }

    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let sess = guard.upload_sessions.get_mut(&id).ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "upload session not found",
        )
    })?;

    sess.status = UploadSessionStatus::Uploading;
    sess.parts.insert(n, body.to_vec());

    let bytes_received: u64 = sess.parts.values().map(|p| p.len() as u64).sum();
    let uploaded_parts: Vec<u32> = sess.parts.keys().copied().collect();
    Ok((
        StatusCode::OK,
        Json(UploadStatusDto {
            session_id: sess.session_id.clone(),
            clip_id: sess.clip_id,
            status: sess.status,
            bytes_received,
            total_bytes: sess.total_bytes,
            uploaded_parts,
            total_parts: sess.total_parts,
            expires_at: Utc::now() + Duration::hours(1),
            clip: None,
        }),
    ))
}

async fn complete_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<UploadStatusDto> {
    let mut guard = state.inner.lock().await;
    let username = require_bearer(&headers, &guard)?;
    let owner_id = guard
        .users_by_name
        .get(&username)
        .map(|u| u.id.clone())
        .unwrap_or_else(|| "usr_unknown".to_string());

    let (clip_id, title, visibility, total_bytes, total_parts, uploaded_parts, assembled) = {
        let sess = guard.upload_sessions.get(&id).ok_or_else(|| {
            api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "upload session not found",
            )
        })?;
        let mut assembled = Vec::with_capacity(sess.total_bytes as usize);
        for part in sess.parts.values() {
            assembled.extend_from_slice(part);
        }
        let uploaded_parts: Vec<u32> = sess.parts.keys().copied().collect();
        (
            sess.clip_id,
            sess.title.clone(),
            sess.visibility,
            sess.total_bytes,
            sess.total_parts,
            uploaded_parts,
            assembled,
        )
    };

    let share_id = format!("shr_{}", &clip_id.simple().to_string()[..12]);
    let now = Utc::now();
    let dto = ClipDto {
        id: clip_id,
        owner_id,
        title,
        game: None,
        duration_ms: 5_000,
        width: 1920,
        height: 1080,
        fps: 60.0,
        size_bytes: assembled.len() as u64,
        codec: "h264".to_string(),
        content_type: "video/mp4".to_string(),
        audio_tracks: vec![],
        tags: vec!["highlight".to_string()],
        notes: None,
        visibility,
        share_id: Some(share_id.clone()),
        share_url: Some(format!("https://clips.example.test/s/{share_id}")),
        stream_url: Some(format!("/api/v1/shares/{share_id}/video")),
        thumbnail_url: None,
        created_at: now,
        uploaded_at: now,
        expires_at: None,
    };

    guard.uploaded_payloads.insert(clip_id, assembled);
    guard.clips.insert(clip_id, dto.clone());
    guard.shares.insert(share_id, clip_id);

    if let Some(sess) = guard.upload_sessions.get_mut(&id) {
        sess.status = UploadSessionStatus::Completed;
        sess.clip = Some(dto.clone());
    }

    Ok((
        StatusCode::OK,
        Json(UploadStatusDto {
            session_id: id,
            clip_id,
            status: UploadSessionStatus::Completed,
            bytes_received: total_bytes,
            total_bytes,
            uploaded_parts,
            total_parts,
            expires_at: now + Duration::hours(1),
            clip: Some(dto),
        }),
    ))
}

async fn abort_upload_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> std::result::Result<StatusCode, (StatusCode, Json<ApiErrorDto>)> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    if guard.upload_sessions.remove(&id).is_none() {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "upload session not found",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_clip_multipart(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<ClipDto> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !content_type.starts_with("multipart/form-data") {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_content_type",
            "expected multipart/form-data",
        ));
    }

    let mut guard = state.inner.lock().await;
    let username = require_bearer(&headers, &guard)?;
    let owner_id = guard
        .users_by_name
        .get(&username)
        .map(|u| u.id.clone())
        .unwrap_or_else(|| "usr_unknown".to_string());

    let raw = body.to_vec();
    let body_text = String::from_utf8_lossy(&raw);
    if !body_text.contains("name=\"metadata\"")
        || !body_text.contains("name=\"visibility\"")
        || !body_text.contains("name=\"file\"")
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_multipart",
            "missing required multipart fields",
        ));
    }

    guard.last_multipart_body = Some(raw.clone());
    let clip_id = Uuid::new_v4();
    let share_id = format!("shr_{}", &clip_id.simple().to_string()[..12]);
    let now = Utc::now();
    let dto = ClipDto {
        id: clip_id,
        owner_id,
        title: "Multipart Fallback Clip".to_string(),
        game: None,
        duration_ms: 3_000,
        width: 1920,
        height: 1080,
        fps: 60.0,
        size_bytes: raw.len() as u64,
        codec: "h264".to_string(),
        content_type: "video/mp4".to_string(),
        audio_tracks: vec![],
        tags: vec![],
        notes: None,
        visibility: ShareVisibility::Public,
        share_id: Some(share_id.clone()),
        share_url: Some(format!("https://clips.example.test/s/{share_id}")),
        stream_url: Some(format!("/api/v1/shares/{share_id}/video")),
        thumbnail_url: None,
        created_at: now,
        uploaded_at: now,
        expires_at: None,
    };

    guard.clips.insert(clip_id, dto.clone());
    guard.shares.insert(share_id, clip_id);

    Ok((StatusCode::CREATED, Json(dto)))
}

#[derive(Deserialize)]
struct ListClipsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_clips(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListClipsQuery>,
) -> ApiResult<Vec<ClipDto>> {
    let guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let mut clips: Vec<ClipDto> = guard.clips.values().cloned().collect();
    clips.sort_by_key(|c| c.uploaded_at);
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(usize::MAX);
    let sliced = clips.into_iter().skip(offset).take(limit).collect();
    Ok((StatusCode::OK, Json(sliced)))
}

async fn get_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> ApiResult<ClipDto> {
    let guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let clip = guard
        .clips
        .get(&id)
        .cloned()
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "clip not found"))?;
    Ok((StatusCode::OK, Json(clip)))
}

async fn update_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<UpdateClipRequest>,
) -> ApiResult<ClipDto> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let clip = guard
        .clips
        .get_mut(&id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "clip not found"))?;

    if let Some(title) = req.title {
        clip.title = title;
    }
    if let Some(notes) = req.notes {
        clip.notes = Some(notes);
    }
    if let Some(tags) = req.tags {
        clip.tags = tags;
    }
    if let Some(visibility) = req.visibility {
        clip.visibility = visibility;
    }
    Ok((StatusCode::OK, Json(clip.clone())))
}

async fn delete_clip(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
) -> std::result::Result<StatusCode, (StatusCode, Json<ApiErrorDto>)> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    if guard.clips.remove(&id).is_none() {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "clip not found",
        ));
    }
    guard.shares.retain(|_, clip_id| *clip_id != id);
    guard.uploaded_payloads.remove(&id);
    Ok(StatusCode::NO_CONTENT)
}

async fn create_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<CreateShareRequest>,
) -> ApiResult<CreateShareResponse> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let clip = guard
        .clips
        .get_mut(&id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "clip not found"))?;

    let share_id = format!("shr_{}", &Uuid::new_v4().simple().to_string()[..12]);
    let now = Utc::now();
    let expires_at = req
        .expires_in_hours
        .map(|h| now + Duration::hours(h as i64));
    let url = format!("https://clips.example.test/s/{share_id}");
    clip.visibility = req.visibility;
    clip.share_id = Some(share_id.clone());
    clip.share_url = Some(url.clone());
    clip.expires_at = expires_at;

    guard.shares.insert(share_id.clone(), id);

    Ok((
        StatusCode::CREATED,
        Json(CreateShareResponse {
            share_id,
            clip_id: id,
            url,
            visibility: req.visibility,
            expires_at,
            created_at: now,
        }),
    ))
}

async fn get_public_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(token): AxumPath<String>,
) -> ApiResult<PublicShareDto> {
    let guard = state.inner.lock().await;
    let clip_id = guard
        .shares
        .get(&token)
        .copied()
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "share not found"))?;
    let clip = guard
        .clips
        .get(&clip_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "clip not found"))?;

    if clip.visibility == ShareVisibility::Private && require_bearer(&headers, &guard).is_err() {
        return Err(api_err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "authentication required for private share",
        ));
    }

    Ok((
        StatusCode::OK,
        Json(PublicShareDto {
            share_id: token.clone(),
            clip_id: clip.id,
            title: clip.title.clone(),
            game: clip.game.clone(),
            duration_ms: clip.duration_ms,
            width: clip.width,
            height: clip.height,
            fps: clip.fps,
            size_bytes: clip.size_bytes,
            codec: clip.codec.clone(),
            content_type: clip.content_type.clone(),
            tags: clip.tags.clone(),
            notes: clip.notes.clone(),
            visibility: clip.visibility,
            url: clip
                .share_url
                .clone()
                .unwrap_or_else(|| format!("https://clips.example.test/s/{token}")),
            video_url: format!("/api/v1/shares/{token}/video"),
            thumbnail_url: clip.thumbnail_url.clone(),
            created_at: clip.created_at,
            uploaded_at: clip.uploaded_at,
            expires_at: clip.expires_at,
        }),
    ))
}

async fn revoke_share(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(token): AxumPath<String>,
) -> std::result::Result<StatusCode, (StatusCode, Json<ApiErrorDto>)> {
    let mut guard = state.inner.lock().await;
    let _username = require_bearer(&headers, &guard)?;
    let clip_id = guard
        .shares
        .remove(&token)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "share not found"))?;
    if let Some(clip) = guard.clips.get_mut(&clip_id) {
        clip.share_id = None;
        clip.share_url = None;
    }
    Ok(StatusCode::NO_CONTENT)
}

fn sample_clip_metadata(clip_id: ClipId, title: &str, file_path: PathBuf) -> ClipMetadata {
    ClipMetadata {
        id: clip_id,
        title: title.to_string(),
        game: Some(GameInfo {
            id: "valorant".to_string(),
            name: "VALORANT".to_string(),
            executable: Some("VALORANT-Win64-Shipping.exe".to_string()),
            process_name: None,
        }),
        file_path,
        thumbnail_path: None,
        created_at: Utc::now(),
        duration_ms: 12_500,
        width: 1920,
        height: 1080,
        fps: 60.0,
        size_bytes: 0,
        codec: "h264".to_string(),
        audio_tracks: vec![],
        tags: vec!["ace".to_string()],
        cloud_share: None,
        notes: Some("Contract test clip".to_string()),
    }
}

#[tokio::test]
async fn health_v1_fallback_and_server_info_contract() {
    let (base_url, state, handle) = spawn_contract_server().await;
    let client = HttpCloudClient::new(&base_url, None);

    // 1. GET /api/v1/health succeeds
    let health = client.health_v1().await.expect("health_v1");
    assert_eq!(health.status, "ok");
    assert!(health.db_ok);
    assert!(health.storage_ok);
    client
        .health_check()
        .await
        .expect("health_check via /api/v1/health");

    // 2. Fallback to GET /health when /api/v1/health returns 404
    state.lock().await.health_v1_404 = true;
    client
        .health_check()
        .await
        .expect("health_check falls back to /health");

    // 3. Unhealthy server surfaces error
    state.lock().await.health_unhealthy = true;
    let err = client
        .health_check()
        .await
        .expect_err("503 should fail health_check");
    assert!(err.to_string().contains("503"));
    assert!(err.to_string().contains("database unavailable"));

    // 4. GET /api/v1/server/info
    let info = client.server_info().await.expect("server_info");
    assert_eq!(info.name, "ClipForge Server");
    assert_eq!(info.api_version, "v1");
    assert_eq!(info.storage_backend, "local");
    assert_eq!(info.chunk_size_bytes, 65_536);
    assert!(info.registration_enabled);
    assert_eq!(
        info.public_base_url.as_deref(),
        Some("https://clips.example.test")
    );

    handle.abort();
}

#[tokio::test]
async fn auth_register_login_me_tokens_and_logout_contract() {
    let (base_url, _state, handle) = spawn_contract_server().await;
    let anon = HttpCloudClient::new(&base_url, None);

    // Register with display name
    let reg = anon
        .register_with_display_name("alice", "hunter2-secret", Some("Alice"))
        .await
        .expect("register");
    assert_eq!(reg.username, "alice");
    assert_eq!(
        reg.user.as_ref().and_then(|u| u.display_name.as_deref()),
        Some("Alice")
    );

    // Wrong password returns 401
    let bad_login = anon
        .login("alice", "wrong")
        .await
        .expect_err("wrong password must fail");
    assert!(bad_login.to_string().contains("401"));
    assert!(bad_login.to_string().contains("invalid_credentials"));

    // Valid login returns new session token
    let login_res = anon.login("alice", "hunter2-secret").await.expect("login");
    let authed = anon.clone().with_token(login_res.token);

    // GET /api/v1/auth/me and GET /api/v1/quota
    let me = authed.current_user().await.expect("current_user");
    assert_eq!(me.username, "alice");
    assert_eq!(me.display_name.as_deref(), Some("Alice"));
    assert!(me.quota.is_some());
    let quota = authed.quota().await.expect("quota");
    assert_eq!(quota.clip_count, 0);

    // POST /api/v1/auth/tokens
    let created_tok = authed
        .create_token(&CreateTokenRequest {
            name: "ci-uploader".to_string(),
            scopes: vec!["clips:write".to_string()],
            expires_in_days: Some(30),
        })
        .await
        .expect("create_token");
    assert_eq!(created_tok.name, "ci-uploader");
    assert!(created_tok.token.starts_with("cf_tok_"));
    assert!(created_tok.expires_at.is_some());

    // GET /api/v1/auth/tokens
    let tokens = authed.list_tokens().await.expect("list_tokens");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].id, created_tok.id);
    assert_eq!(tokens[0].scopes, vec!["clips:write"]);

    // DELETE /api/v1/auth/tokens/{id}
    authed
        .revoke_token(&created_tok.id)
        .await
        .expect("revoke_token");
    assert!(authed.list_tokens().await.expect("list_tokens").is_empty());

    // POST /api/v1/auth/logout revokes session token
    authed.logout().await.expect("logout");
    let after_logout = authed
        .current_user()
        .await
        .expect_err("revoked session token must fail");
    assert!(after_logout.to_string().contains("401"));

    handle.abort();
}

#[tokio::test]
async fn chunked_upload_multipart_fallback_and_session_abort_contract() {
    let (base_url, state, handle) = spawn_contract_server().await;
    let anon = HttpCloudClient::new(&base_url, None);
    let auth = anon
        .register("uploader", "pass1234")
        .await
        .expect("register");
    let client = anon.with_token(auth.token);

    // 1. Multi-part chunked upload (> 2 * 65_536 bytes = 3 parts)
    let payload: Vec<u8> = (0..140_000usize).map(|i| (i % 251) as u8).collect();
    let tmp_path =
        std::env::temp_dir().join(format!("clipforge-contract-chunked-{}.mp4", Uuid::new_v4()));
    tokio::fs::write(&tmp_path, &payload)
        .await
        .expect("write temp file");

    let clip_id = ClipId::new();
    let meta = sample_clip_metadata(clip_id, "Multi-Part Clip", tmp_path.clone());
    let mut progress = Vec::new();
    let summary = client
        .upload_clip_streaming(&meta, &tmp_path, ShareVisibility::Public, |p| {
            progress.push(p);
        })
        .await
        .expect("chunked upload");
    assert_eq!(summary.clip_id, clip_id);
    assert_eq!(summary.visibility, ShareVisibility::Public);
    assert_eq!(progress.last(), Some(&100));
    assert!(progress.len() >= 4); // 3 parts + completion 100

    let stored = state
        .lock()
        .await
        .uploaded_payloads
        .get(&clip_id.0)
        .cloned()
        .expect("stored payload");
    assert_eq!(stored, payload);

    // 2. Direct session status & abort contract
    let sess = client
        .create_upload_session(&CreateUploadSessionRequest {
            clip_id: None,
            title: "Aborted".to_string(),
            filename: "aborted.mp4".to_string(),
            total_bytes: 1024,
            content_type: Some("video/mp4".to_string()),
            sha256: None,
            visibility: ShareVisibility::Private,
            game: None,
            duration_ms: 0,
            width: 0,
            height: 0,
            fps: 0.0,
            codec: String::new(),
            audio_tracks: vec![],
            tags: vec![],
            notes: None,
            created_at: None,
        })
        .await
        .expect("create_upload_session");
    let status = client
        .upload_session_status(&sess.session_id)
        .await
        .expect("upload_session_status");
    assert_eq!(status.status, UploadSessionStatus::Pending);
    client
        .abort_upload_session(&sess.session_id)
        .await
        .expect("abort_upload_session");
    let after_abort = client
        .upload_session_status(&sess.session_id)
        .await
        .expect_err("aborted session must 404");
    assert!(matches!(after_abort, ClipForgeError::NotFound(_)));

    // 3. 404 on POST /api/v1/uploads falls back to multipart POST /api/v1/clips
    state.lock().await.uploads_404 = true;
    let mut fallback_progress = Vec::new();
    let fallback_summary = client
        .upload_clip_streaming(&meta, &tmp_path, ShareVisibility::Private, |p| {
            fallback_progress.push(p);
        })
        .await
        .expect("multipart fallback upload");
    assert_eq!(fallback_progress, vec![10, 100]);
    assert!(!fallback_summary.share_id.is_empty());
    assert!(state.lock().await.last_multipart_body.is_some());

    let _ = tokio::fs::remove_file(&tmp_path).await;
    handle.abort();
}

#[tokio::test]
async fn clips_crud_visibility_and_shares_contract() {
    let (base_url, _state, handle) = spawn_contract_server().await;
    let anon = HttpCloudClient::new(&base_url, None);
    let auth = anon.register("sharer", "pass1234").await.expect("register");
    let client = anon.clone().with_token(auth.token);

    let tmp_path =
        std::env::temp_dir().join(format!("clipforge-contract-crud-{}.mp4", Uuid::new_v4()));
    tokio::fs::write(&tmp_path, b"video-bytes-123")
        .await
        .expect("write temp file");

    let clip_id = ClipId::new();
    let meta = sample_clip_metadata(clip_id, "Original Title", tmp_path.clone());
    let uploaded = client
        .upload_clip_streaming(&meta, &tmp_path, ShareVisibility::Public, |_| {})
        .await
        .expect("upload");
    let _ = tokio::fs::remove_file(&tmp_path).await;

    // GET /api/v1/clips?limit=10&offset=0
    let list_v1 = client
        .list_clips_v1(Some(10), Some(0))
        .await
        .expect("list_clips_v1");
    assert_eq!(list_v1.len(), 1);
    assert_eq!(list_v1[0].id, clip_id.0);

    let list_summary = client
        .list_clips(Some(10), Some(0))
        .await
        .expect("list_clips");
    assert_eq!(list_summary.len(), 1);
    assert_eq!(list_summary[0].share_id, uploaded.share_id);

    // PATCH /api/v1/clips/{id} metadata & visibility
    let patched = client
        .update_clip(
            &clip_id.to_string(),
            &UpdateClipRequest {
                title: Some("Updated Title".to_string()),
                notes: Some("Edited notes".to_string()),
                tags: Some(vec!["edited".to_string()]),
                visibility: Some(ShareVisibility::Private),
            },
        )
        .await
        .expect("update_clip");
    assert_eq!(patched.title, "Updated Title");
    assert_eq!(patched.visibility, ShareVisibility::Private);

    // Unauthenticated GET /api/v1/shares/{token} fails while private
    let unauth_err = anon
        .get_public_share(&uploaded.share_id)
        .await
        .expect_err("private share requires auth");
    assert!(unauth_err.to_string().contains("401"));

    // set_visibility back to Public
    let vis_summary = client
        .set_visibility(&clip_id.to_string(), ShareVisibility::Public)
        .await
        .expect("set_visibility");
    assert_eq!(vis_summary.visibility, ShareVisibility::Public);

    // POST /api/v1/clips/{id}/shares
    let new_share = client
        .create_share(
            &clip_id.to_string(),
            &CreateShareRequest {
                visibility: ShareVisibility::Public,
                expires_in_hours: Some(48),
            },
        )
        .await
        .expect("create_share");
    assert!(new_share.expires_at.is_some());

    // GET /api/v1/shares/{token} (unauthenticated public access + CloudClient::get_share)
    let public_dto = anon
        .get_public_share(&new_share.share_id)
        .await
        .expect("get_public_share");
    assert_eq!(public_dto.clip_id, clip_id.0);
    assert_eq!(public_dto.title, "Updated Title");
    assert_eq!(
        public_dto.video_url,
        format!("/api/v1/shares/{}/video", new_share.share_id)
    );

    let cloud_share = anon
        .get_share(&new_share.share_id)
        .await
        .expect("CloudClient::get_share");
    assert_eq!(cloud_share.share_id, new_share.share_id);

    // DELETE /api/v1/shares/{id}
    client
        .revoke_share(&new_share.share_id)
        .await
        .expect("revoke_share");
    let after_revoke = anon
        .get_public_share(&new_share.share_id)
        .await
        .expect_err("revoked share must 404");
    assert!(matches!(after_revoke, ClipForgeError::NotFound(_)));

    // DELETE /api/v1/clips/{id}
    client
        .delete_clip(&clip_id.to_string())
        .await
        .expect("delete_clip");
    let after_delete = client
        .get_clip(&clip_id.to_string())
        .await
        .expect_err("deleted clip must 404");
    assert!(matches!(after_delete, ClipForgeError::NotFound(_)));

    handle.abort();
}

#[tokio::test]
async fn payload_too_large_413_and_api_error_dto_parsing() {
    let (base_url, state, handle) = spawn_contract_server().await;
    state.lock().await.max_upload_bytes = 32;

    let anon = HttpCloudClient::new(&base_url, None);
    let auth = anon
        .register("limited", "pass1234")
        .await
        .expect("register");
    let client = anon.clone().with_token(auth.token);

    let tmp_path =
        std::env::temp_dir().join(format!("clipforge-contract-413-{}.mp4", Uuid::new_v4()));
    tokio::fs::write(&tmp_path, vec![9u8; 128])
        .await
        .expect("write temp file");

    let meta = sample_clip_metadata(ClipId::new(), "Too Large", tmp_path.clone());
    let err = client
        .upload_clip_streaming(&meta, &tmp_path, ShareVisibility::Public, |_| {})
        .await
        .expect_err("413 must fail without multipart fallback");
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let msg = err.to_string();
    assert!(matches!(err, ClipForgeError::Cloud(_)));
    assert!(msg.contains("413"), "expected 413 in error: {msg}");
    assert!(
        msg.contains("exceeds max_upload_bytes"),
        "expected ApiErrorDto.error in message: {msg}"
    );
    assert!(
        msg.contains("payload_too_large"),
        "expected ApiErrorDto.code in message: {msg}"
    );
    assert!(
        state.lock().await.last_multipart_body.is_none(),
        "413 must not trigger 404 multipart fallback"
    );

    // Verify 400 Bad Request ApiErrorDto maps to ClipForgeError::InvalidInput
    let bad_req_err = client
        .create_token(&CreateTokenRequest {
            name: "   ".to_string(),
            scopes: vec![],
            expires_in_days: None,
        })
        .await
        .expect_err("empty token name must 400");
    assert!(matches!(bad_req_err, ClipForgeError::InvalidInput(_)));
    let bad_msg = bad_req_err.to_string();
    assert!(bad_msg.contains("token name cannot be empty"));
    assert!(bad_msg.contains("invalid_token_name"));

    // Duplicate registration returns 409 with ApiErrorDto
    let dup_err = anon
        .register("limited", "pass1234")
        .await
        .expect_err("duplicate register must 409");
    let dup_msg = dup_err.to_string();
    assert!(dup_msg.contains("409"));
    assert!(dup_msg.contains("username_taken"));

    handle.abort();
}
