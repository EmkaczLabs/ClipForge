use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use clipforge_common::{AudioTrackInfo, ClipId, GameInfo};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::hash_token;
use crate::error::ServerError;

/// Visibility level of a shared clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }

    pub fn parse(s: &str) -> Result<Self, ServerError> {
        match s.to_ascii_lowercase().as_str() {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            _ => Err(ServerError::BadRequest(format!(
                "invalid visibility '{s}'; expected 'public' or 'private'"
            ))),
        }
    }
}

/// Authenticated user identity.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub token_hash: String,
}

/// Public API key metadata (never includes raw secret).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeySummary {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Full clip & share record stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredClip {
    pub clip_id: ClipId,
    pub owner_id: String,
    pub share_id: String,
    pub title: String,
    pub game: Option<GameInfo>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub size_bytes: u64,
    pub codec: String,
    pub audio_tracks: Vec<AudioTrackInfo>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub storage_key: String,
    pub content_type: String,
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub uploaded_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Parameters for inserting a newly uploaded clip.
#[derive(Debug, Clone)]
pub struct NewClipParams {
    pub clip_id: ClipId,
    pub owner_id: String,
    pub share_id: String,
    pub title: String,
    pub game: Option<GameInfo>,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub size_bytes: u64,
    pub codec: String,
    pub audio_tracks: Vec<AudioTrackInfo>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub storage_key: String,
    pub content_type: String,
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// SQLite-backed persistence layer for users, tokens, API keys, and clips.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens (or creates) the SQLite database at `path` and runs schema migrations.
    pub fn open(path: &Path) -> Result<Self, ServerError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory SQLite database (useful for tests).
    pub fn open_in_memory() -> Result<Self, ServerError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::migrate(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn migrate(conn: &Connection) -> Result<(), ServerError> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                token_hash TEXT NOT NULL UNIQUE,
                expires_at TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                key_prefix TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                revoked_at TEXT
            );

            CREATE TABLE IF NOT EXISTS clips (
                clip_id TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                share_id TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                game_json TEXT,
                duration_ms INTEGER NOT NULL,
                width INTEGER NOT NULL,
                height INTEGER NOT NULL,
                fps REAL NOT NULL,
                size_bytes INTEGER NOT NULL,
                codec TEXT NOT NULL,
                audio_tracks_json TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                notes TEXT,
                storage_key TEXT NOT NULL,
                content_type TEXT NOT NULL,
                visibility TEXT NOT NULL CHECK(visibility IN ('public', 'private')),
                created_at TEXT NOT NULL,
                uploaded_at TEXT NOT NULL,
                expires_at TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_clips_owner ON clips(owner_id, uploaded_at DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_share ON clips(share_id);
            ",
        )?;
        Ok(())
    }

    async fn run<F, T>(&self, f: F) -> Result<T, ServerError>
    where
        F: FnOnce(&Connection) -> Result<T, ServerError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn
                .lock()
                .map_err(|_| ServerError::Internal("database mutex poisoned".to_string()))?;
            f(&guard)
        })
        .await
        .map_err(|e| ServerError::Internal(format!("database task panicked: {e}")))?
    }

    /// Creates a new user and returns their user ID.
    pub async fn create_user(
        &self,
        username: String,
        password_hash: String,
    ) -> Result<String, ServerError> {
        let user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let uid = user_id.clone();

        self.run(move |conn| {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT id FROM users WHERE username = ?1",
                    params![username],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_some() {
                return Err(ServerError::Conflict("username already exists".to_string()));
            }
            conn.execute(
                "INSERT INTO users (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![uid, username, password_hash, now],
            )?;
            Ok(())
        })
        .await?;

        Ok(user_id)
    }

    /// Fetches `(user_id, password_hash)` for a username.
    pub async fn get_user_by_username(
        &self,
        username: String,
    ) -> Result<Option<(String, String)>, ServerError> {
        self.run(move |conn| {
            conn.query_row(
                "SELECT id, password_hash FROM users WHERE username = ?1",
                params![username],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
        })
        .await
    }

    /// Stores a hashed session bearer token for `user_id`.
    pub async fn create_session(
        &self,
        user_id: String,
        raw_token: &str,
        ttl_hours: i64,
    ) -> Result<DateTime<Utc>, ServerError> {
        let id = Uuid::new_v4().to_string();
        let token_hash = hash_token(raw_token);
        let now = Utc::now();
        let expires_at = now + Duration::hours(ttl_hours.max(1));
        let now_s = now.to_rfc3339();
        let exp_s = expires_at.to_rfc3339();

        self.run(move |conn| {
            conn.execute(
                "INSERT INTO sessions (id, user_id, token_hash, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, user_id, token_hash, exp_s, now_s],
            )?;
            Ok(())
        })
        .await?;

        Ok(expires_at)
    }

    /// Revokes a session by its hashed token.
    pub async fn revoke_session(&self, token_hash: String) -> Result<(), ServerError> {
        self.run(move |conn| {
            conn.execute(
                "DELETE FROM sessions WHERE token_hash = ?1",
                params![token_hash],
            )?;
            Ok(())
        })
        .await
    }

    /// Creates a hashed API key for `user_id`.
    pub async fn create_api_key(
        &self,
        user_id: String,
        name: String,
        raw_key: &str,
    ) -> Result<ApiKeySummary, ServerError> {
        let id = Uuid::new_v4().to_string();
        let key_hash = hash_token(raw_key);
        let key_prefix: String = raw_key.chars().take(12).collect();
        let created_at = Utc::now();
        let created_s = created_at.to_rfc3339();

        let summary = ApiKeySummary {
            id: id.clone(),
            name: name.clone(),
            key_prefix: key_prefix.clone(),
            created_at,
            revoked_at: None,
        };

        self.run(move |conn| {
            conn.execute(
                "INSERT INTO api_keys (id, user_id, name, key_prefix, key_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, user_id, name, key_prefix, key_hash, created_s],
            )?;
            Ok(())
        })
        .await?;

        Ok(summary)
    }

    /// Lists non-revoked and revoked API keys for `user_id`.
    pub async fn list_api_keys(&self, user_id: String) -> Result<Vec<ApiKeySummary>, ServerError> {
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, key_prefix, created_at, revoked_at FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map(params![user_id], |row| {
                let created_s: String = row.get(3)?;
                let revoked_s: Option<String> = row.get(4)?;
                Ok(ApiKeySummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    key_prefix: row.get(2)?,
                    created_at: parse_dt(&created_s),
                    revoked_at: revoked_s.as_deref().map(parse_dt),
                })
            })?;
            let mut out = Vec::new();
            for item in rows {
                out.push(item?);
            }
            Ok(out)
        })
        .await
    }

    /// Revokes an API key owned by `user_id`.
    pub async fn revoke_api_key(
        &self,
        user_id: String,
        key_id: String,
    ) -> Result<bool, ServerError> {
        let now = Utc::now().to_rfc3339();
        self.run(move |conn| {
            let count = conn.execute(
                "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
                params![now, key_id, user_id],
            )?;
            Ok(count > 0)
        })
        .await
    }

    /// Authenticates a raw bearer token or API key by its SHA-256 hash.
    pub async fn authenticate_credential(
        &self,
        raw_secret: &str,
    ) -> Result<Option<AuthUser>, ServerError> {
        let token_hash = hash_token(raw_secret);
        let now_s = Utc::now().to_rfc3339();

        self.run(move |conn| {
            // Check active sessions first
            let session_user: Option<(String, String)> = conn
                .query_row(
                    "SELECT u.id, u.username
                     FROM sessions s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.token_hash = ?1 AND s.expires_at > ?2",
                    params![token_hash, now_s],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            if let Some((user_id, username)) = session_user {
                return Ok(Some(AuthUser {
                    user_id,
                    username,
                    token_hash,
                }));
            }

            // Check active API keys
            let api_user: Option<(String, String)> = conn
                .query_row(
                    "SELECT u.id, u.username
                     FROM api_keys k
                     JOIN users u ON u.id = k.user_id
                     WHERE k.key_hash = ?1 AND k.revoked_at IS NULL",
                    params![token_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;

            Ok(api_user.map(|(user_id, username)| AuthUser {
                user_id,
                username,
                token_hash,
            }))
        })
        .await
    }

    /// Seeds a bootstrap user + API key if no users exist yet (used when `CLIPFORGE_BOOTSTRAP_API_KEY` is set).
    pub async fn ensure_bootstrap_api_key(&self, raw_key: &str) -> Result<(), ServerError> {
        let key_hash = hash_token(raw_key);
        let key_prefix: String = raw_key.chars().take(12).collect();
        let now_s = Utc::now().to_rfc3339();
        let user_id = Uuid::new_v4().to_string();
        let key_id = Uuid::new_v4().to_string();

        self.run(move |conn| {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT id FROM api_keys WHERE key_hash = ?1",
                    params![key_hash],
                    |row| row.get(0),
                )
                .optional()?;
            if existing.is_some() {
                return Ok(());
            }

            let admin_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM users WHERE username = 'bootstrap'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let uid = match admin_id {
                Some(id) => id,
                None => {
                    conn.execute(
                        "INSERT INTO users (id, username, password_hash, created_at) VALUES (?1, 'bootstrap', 'disabled', ?2)",
                        params![user_id, now_s],
                    )?;
                    user_id
                }
            };

            conn.execute(
                "INSERT INTO api_keys (id, user_id, name, key_prefix, key_hash, created_at) VALUES (?1, ?2, 'bootstrap', ?3, ?4, ?5)",
                params![key_id, uid, key_prefix, key_hash, now_s],
            )?;
            Ok(())
        })
        .await
    }

    /// Inserts or replaces a clip record.
    pub async fn insert_clip(&self, p: NewClipParams) -> Result<StoredClip, ServerError> {
        let uploaded_at = Utc::now();
        let game_json = p
            .game
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| ServerError::Internal(format!("serialize game: {e}")))?;
        let audio_json = serde_json::to_string(&p.audio_tracks)
            .map_err(|e| ServerError::Internal(format!("serialize audio_tracks: {e}")))?;
        let tags_json = serde_json::to_string(&p.tags)
            .map_err(|e| ServerError::Internal(format!("serialize tags: {e}")))?;

        let clip = StoredClip {
            clip_id: p.clip_id,
            owner_id: p.owner_id.clone(),
            share_id: p.share_id.clone(),
            title: p.title.clone(),
            game: p.game.clone(),
            duration_ms: p.duration_ms,
            width: p.width,
            height: p.height,
            fps: p.fps,
            size_bytes: p.size_bytes,
            codec: p.codec.clone(),
            audio_tracks: p.audio_tracks.clone(),
            tags: p.tags.clone(),
            notes: p.notes.clone(),
            storage_key: p.storage_key.clone(),
            content_type: p.content_type.clone(),
            visibility: p.visibility,
            created_at: p.created_at,
            uploaded_at,
            expires_at: p.expires_at,
        };

        let clip_id_s = p.clip_id.to_string();
        let created_s = p.created_at.to_rfc3339();
        let uploaded_s = uploaded_at.to_rfc3339();
        let expires_s = p.expires_at.map(|d| d.to_rfc3339());
        let vis_s = p.visibility.as_str().to_string();

        self.run(move |conn| {
            conn.execute(
                "INSERT INTO clips (
                    clip_id, owner_id, share_id, title, game_json,
                    duration_ms, width, height, fps, size_bytes, codec,
                    audio_tracks_json, tags_json, notes, storage_key,
                    content_type, visibility, created_at, uploaded_at, expires_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                params![
                    clip_id_s,
                    p.owner_id,
                    p.share_id,
                    p.title,
                    game_json,
                    p.duration_ms as i64,
                    p.width as i64,
                    p.height as i64,
                    p.fps as f64,
                    p.size_bytes as i64,
                    p.codec,
                    audio_json,
                    tags_json,
                    p.notes,
                    p.storage_key,
                    p.content_type,
                    vis_s,
                    created_s,
                    uploaded_s,
                    expires_s,
                ],
            )?;
            Ok(())
        })
        .await?;

        Ok(clip)
    }

    /// Lists clips owned by `owner_id`.
    pub async fn list_clips(
        &self,
        owner_id: String,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<StoredClip>, ServerError> {
        let limit = limit.clamp(1, 200) as i64;
        let offset = offset as i64;
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT clip_id, owner_id, share_id, title, game_json,
                        duration_ms, width, height, fps, size_bytes, codec,
                        audio_tracks_json, tags_json, notes, storage_key,
                        content_type, visibility, created_at, uploaded_at, expires_at
                 FROM clips
                 WHERE owner_id = ?1
                 ORDER BY uploaded_at DESC
                 LIMIT ?2 OFFSET ?3",
            )?;
            let rows = stmt.query_map(params![owner_id, limit, offset], row_to_stored_clip)?;
            let mut out = Vec::new();
            for item in rows {
                out.push(item?);
            }
            Ok(out)
        })
        .await
    }

    /// Finds a clip by either `share_id` or `clip_id`.
    pub async fn get_by_share_or_clip_id(
        &self,
        id: String,
    ) -> Result<Option<StoredClip>, ServerError> {
        self.run(move |conn| {
            conn.query_row(
                "SELECT clip_id, owner_id, share_id, title, game_json,
                        duration_ms, width, height, fps, size_bytes, codec,
                        audio_tracks_json, tags_json, notes, storage_key,
                        content_type, visibility, created_at, uploaded_at, expires_at
                 FROM clips
                 WHERE share_id = ?1 OR clip_id = ?1",
                params![id],
                row_to_stored_clip,
            )
            .optional()
            .map_err(Into::into)
        })
        .await
    }

    /// Updates visibility for a clip owned by `owner_id`.
    pub async fn set_visibility(
        &self,
        id: String,
        owner_id: String,
        visibility: Visibility,
    ) -> Result<Option<StoredClip>, ServerError> {
        let vis_s = visibility.as_str().to_string();
        let id_clone = id.clone();
        let updated = self
            .run(move |conn| {
                let count = conn.execute(
                    "UPDATE clips SET visibility = ?1 WHERE (share_id = ?2 OR clip_id = ?2) AND owner_id = ?3",
                    params![vis_s, id_clone, owner_id],
                )?;
                Ok(count > 0)
            })
            .await?;

        if !updated {
            return Ok(None);
        }
        self.get_by_share_or_clip_id(id).await
    }

    /// Deletes a clip owned by `owner_id`, returning the deleted record if it existed.
    pub async fn delete_clip(
        &self,
        id: String,
        owner_id: String,
    ) -> Result<Option<StoredClip>, ServerError> {
        let existing = self.get_by_share_or_clip_id(id.clone()).await?;
        let Some(clip) = existing else {
            return Ok(None);
        };
        if clip.owner_id != owner_id {
            return Err(ServerError::Forbidden(
                "you do not own this clip".to_string(),
            ));
        }

        let clip_id_s = clip.clip_id.to_string();
        self.run(move |conn| {
            conn.execute("DELETE FROM clips WHERE clip_id = ?1", params![clip_id_s])?;
            Ok(())
        })
        .await?;

        Ok(Some(clip))
    }
}

fn row_to_stored_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredClip> {
    let clip_id_s: String = row.get(0)?;
    let game_json: Option<String> = row.get(4)?;
    let audio_json: String = row.get(11)?;
    let tags_json: String = row.get(12)?;
    let vis_s: String = row.get(16)?;
    let created_s: String = row.get(17)?;
    let uploaded_s: String = row.get(18)?;
    let expires_s: Option<String> = row.get(19)?;

    let clip_id = clip_id_s
        .parse::<ClipId>()
        .unwrap_or_else(|_| ClipId::new());
    let game = game_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<GameInfo>(s).ok());
    let audio_tracks = serde_json::from_str::<Vec<AudioTrackInfo>>(&audio_json).unwrap_or_default();
    let tags = serde_json::from_str::<Vec<String>>(&tags_json).unwrap_or_default();
    let visibility = Visibility::parse(&vis_s).unwrap_or(Visibility::Public);

    Ok(StoredClip {
        clip_id,
        owner_id: row.get(1)?,
        share_id: row.get(2)?,
        title: row.get(3)?,
        game,
        duration_ms: row.get::<_, i64>(5)? as u64,
        width: row.get::<_, i64>(6)? as u32,
        height: row.get::<_, i64>(7)? as u32,
        fps: row.get::<_, f64>(8)? as f32,
        size_bytes: row.get::<_, i64>(9)? as u64,
        codec: row.get(10)?,
        audio_tracks,
        tags,
        notes: row.get(13)?,
        storage_key: row.get(14)?,
        content_type: row.get(15)?,
        visibility,
        created_at: parse_dt(&created_s),
        uploaded_at: parse_dt(&uploaded_s),
        expires_at: expires_s.as_deref().map(parse_dt),
    })
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
