use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clipforge_common::{
    AudioTrackInfo, ClipFilter, ClipForgeError, ClipId, ClipMetadata, ClipStore, CloudShareInfo,
    GameInfo, Result,
};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};

use crate::migrations::{apply_migrations, current_schema_version};

/// Origin classification for a clip stored in [`SqliteClipStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ClipOrigin {
    /// Captured via screen/replay recording.
    #[default]
    Recorded,
    /// Imported from an external file on disk.
    Imported,
    /// Created by trimming an existing clip.
    Trimmed,
    /// Built-in sample/demo clip (not a user recording).
    Sample,
}

impl ClipOrigin {
    /// Returns the canonical database representation for this origin.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recorded => "recorded",
            Self::Imported => "imported",
            Self::Trimmed => "trimmed",
            Self::Sample => "sample",
        }
    }

    /// Returns `true` if this clip is a sample/demo entry rather than a user recording.
    pub fn is_sample(self) -> bool {
        matches!(self, Self::Sample)
    }
}

impl FromStr for ClipOrigin {
    type Err = ClipForgeError;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "recorded" => Ok(Self::Recorded),
            "imported" => Ok(Self::Imported),
            "trimmed" => Ok(Self::Trimmed),
            "sample" => Ok(Self::Sample),
            other => Err(ClipForgeError::InvalidInput(format!(
                "unknown clip origin: {other}"
            ))),
        }
    }
}

/// Options for listing clips from [`SqliteClipStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipListOptions {
    pub sort: ClipSort,
    pub favorites_only: bool,
    pub origin: Option<ClipOrigin>,
    pub exclude_samples: bool,
}

/// Sort order for listing clips from [`SqliteClipStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipSort {
    #[default]
    CreatedDesc,
    CreatedAsc,
    DurationDesc,
    DurationAsc,
    TitleAsc,
    SizeDesc,
}

impl ClipSort {
    fn order_by_clause(self) -> &'static str {
        match self {
            Self::CreatedDesc => "c.created_at DESC, c.id ASC",
            Self::CreatedAsc => "c.created_at ASC, c.id ASC",
            Self::DurationDesc => "c.duration_ms DESC, c.created_at DESC",
            Self::DurationAsc => "c.duration_ms ASC, c.created_at DESC",
            Self::TitleAsc => "c.title COLLATE NOCASE ASC, c.created_at DESC",
            Self::SizeDesc => "c.size_bytes DESC, c.created_at DESC",
        }
    }
}

/// SQLite-backed implementation of [`ClipStore`] using `rusqlite` (`bundled`).
#[derive(Debug, Clone)]
pub struct SqliteClipStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteClipStore {
    /// Opens (or creates) a SQLite clip library database at `path` and runs migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).map_err(db_err)?;
        configure_connection(&conn, true)?;
        apply_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an isolated in-memory SQLite clip library database and runs migrations.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        configure_connection(&conn, false)?;
        apply_migrations(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Returns the current applied schema migration version.
    pub async fn schema_version(&self) -> Result<u32> {
        self.with_conn(|conn| current_schema_version(conn)).await
    }

    /// Lists clips with explicit [`ClipSort`] and optional `favorites_only` filtering.
    pub async fn list_clips_with_options(
        &self,
        filter: &ClipFilter,
        sort: ClipSort,
        favorites_only: bool,
    ) -> Result<Vec<ClipMetadata>> {
        self.list_clips_filtered(
            filter,
            ClipListOptions {
                sort,
                favorites_only,
                origin: None,
                exclude_samples: false,
            },
        )
        .await
    }

    /// Lists clips with [`ClipListOptions`] (sort, favorites, origin, sample exclusion).
    pub async fn list_clips_filtered(
        &self,
        filter: &ClipFilter,
        options: ClipListOptions,
    ) -> Result<Vec<ClipMetadata>> {
        let filter = filter.clone();
        self.with_conn(move |conn| list_clips_sync(conn, &filter, options))
            .await
    }

    /// Inserts or updates a clip and explicitly sets its [`ClipOrigin`].
    pub async fn upsert_clip_with_origin(
        &self,
        clip: &ClipMetadata,
        origin: ClipOrigin,
    ) -> Result<()> {
        let clip = clip.clone();
        self.with_conn(move |conn| upsert_clip_sync(conn, &clip, Some(origin)))
            .await
    }

    /// Updates the [`ClipOrigin`] of an existing clip. Returns `true` if the clip exists.
    pub async fn set_origin(&self, id: ClipId, origin: ClipOrigin) -> Result<bool> {
        let id_str = id.to_string();
        let origin_str = origin.as_str();
        self.with_conn(move |conn| {
            let changed = conn
                .execute(
                    "UPDATE clips SET origin = ?1 WHERE id = ?2",
                    params![origin_str, id_str],
                )
                .map_err(db_err)?;
            Ok(changed > 0)
        })
        .await
    }

    /// Returns the [`ClipOrigin`] for the given clip.
    pub async fn get_origin(&self, id: ClipId) -> Result<ClipOrigin> {
        let id_str = id.to_string();
        self.with_conn(move |conn| {
            let raw: Option<String> = conn
                .query_row(
                    "SELECT origin FROM clips WHERE id = ?1",
                    params![id_str],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;

            match raw {
                Some(s) => s.parse(),
                None => Err(ClipForgeError::NotFound(format!("clip not found: {id}"))),
            }
        })
        .await
    }

    /// Returns a map of all clip IDs to their [`ClipOrigin`].
    pub async fn list_origins(&self) -> Result<HashMap<ClipId, ClipOrigin>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT id, origin FROM clips")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(db_err)?;
            let mut map = HashMap::new();
            for row in rows {
                let (id_str, origin_str) = row.map_err(db_err)?;
                if let (Ok(id), Ok(origin)) = (id_str.parse::<ClipId>(), origin_str.parse()) {
                    map.insert(id, origin);
                }
            }
            Ok(map)
        })
        .await
    }

    /// Marks or unmarks a clip as a favorite. Returns `true` if the clip exists.
    pub async fn set_favorite(&self, id: ClipId, favorite: bool) -> Result<bool> {
        let id_str = id.to_string();
        let fav_int: i64 = i64::from(favorite);
        self.with_conn(move |conn| {
            let changed = conn
                .execute(
                    "UPDATE clips SET is_favorite = ?1 WHERE id = ?2",
                    params![fav_int, id_str],
                )
                .map_err(db_err)?;
            Ok(changed > 0)
        })
        .await
    }

    /// Returns whether the given clip is marked as a favorite.
    pub async fn is_favorite(&self, id: ClipId) -> Result<bool> {
        let id_str = id.to_string();
        self.with_conn(move |conn| {
            let fav: Option<i64> = conn
                .query_row(
                    "SELECT is_favorite FROM clips WHERE id = ?1",
                    params![id_str],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;

            match fav {
                Some(v) => Ok(v != 0),
                None => Err(ClipForgeError::NotFound(format!("clip not found: {id}"))),
            }
        })
        .await
    }

    /// Associates (or clears) game metadata on an existing clip. Returns `true` if the clip exists.
    pub async fn set_game(&self, id: ClipId, game: Option<&GameInfo>) -> Result<bool> {
        let id_str = id.to_string();
        let game = game.cloned();
        self.with_conn(move |conn| {
            let (gid, gname, gexe, gproc) = match &game {
                Some(g) => (
                    Some(g.id.as_str()),
                    Some(g.name.as_str()),
                    g.executable.as_deref(),
                    g.process_name.as_deref(),
                ),
                None => (None, None, None, None),
            };
            let changed = conn
                .execute(
                    "UPDATE clips
                     SET game_id = ?1, game_name = ?2, game_executable = ?3, game_process_name = ?4
                     WHERE id = ?5",
                    params![gid, gname, gexe, gproc, id_str],
                )
                .map_err(db_err)?;
            Ok(changed > 0)
        })
        .await
    }

    /// Looks up a clip by its file path (falling back to canonicalized path if the file exists).
    pub async fn get_by_file_path(&self, path: &Path) -> Result<Option<ClipMetadata>> {
        let path_str = path.to_string_lossy().into_owned();
        let canon_str = std::fs::canonicalize(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .filter(|c| c != &path_str);
        self.with_conn(move |conn| {
            let mut stmt = conn
                .prepare(&format!("{SELECT_CLIP_COLUMNS} WHERE c.file_path = ?1"))
                .map_err(db_err)?;
            let mut clip = stmt
                .query_row(params![path_str], row_to_clip_base)
                .optional()
                .map_err(db_err)?;
            if clip.is_none()
                && let Some(canon) = canon_str
            {
                clip = stmt
                    .query_row(params![canon], row_to_clip_base)
                    .optional()
                    .map_err(db_err)?;
            }
            match clip {
                Some(mut c) => {
                    c.tags = load_tags_for_clip(conn, &c.id.to_string())?;
                    Ok(Some(c))
                }
                None => Ok(None),
            }
        })
        .await
    }

    /// Returns the set of all file paths currently recorded in the library.
    pub async fn known_file_paths(&self) -> Result<HashSet<PathBuf>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT file_path FROM clips")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(db_err)?;
            let mut set = HashSet::new();
            for row in rows {
                let raw = row.map_err(db_err)?;
                set.insert(PathBuf::from(raw));
            }
            Ok(set)
        })
        .await
    }

    async fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| ClipForgeError::Database("database mutex poisoned".into()))?;
            f(&mut guard)
        })
        .await
        .map_err(|e| ClipForgeError::Database(format!("database task panicked: {e}")))?
    }
}

#[async_trait]
impl ClipStore for SqliteClipStore {
    async fn upsert_clip(&self, clip: &ClipMetadata) -> Result<()> {
        let clip = clip.clone();
        self.with_conn(move |conn| upsert_clip_sync(conn, &clip, None))
            .await
    }

    async fn get_clip(&self, id: ClipId) -> Result<Option<ClipMetadata>> {
        let id_str = id.to_string();
        self.with_conn(move |conn| get_clip_sync(conn, &id_str))
            .await
    }

    async fn list_clips(&self, filter: &ClipFilter) -> Result<Vec<ClipMetadata>> {
        self.list_clips_with_options(filter, ClipSort::CreatedDesc, false)
            .await
    }

    async fn delete_clip(&self, id: ClipId) -> Result<bool> {
        let id_str = id.to_string();
        self.with_conn(move |conn| {
            let changed = conn
                .execute("DELETE FROM clips WHERE id = ?1", params![id_str])
                .map_err(db_err)?;
            Ok(changed > 0)
        })
        .await
    }

    async fn add_tag(&self, id: ClipId, tag: &str) -> Result<()> {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(ClipForgeError::InvalidInput("tag cannot be empty".into()));
        }
        let id_str = id.to_string();
        let tag_owned = trimmed.to_string();
        self.with_conn(move |conn| {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT 1 FROM clips WHERE id = ?1",
                    params![id_str],
                    |row| row.get(0),
                )
                .optional()
                .map_err(db_err)?;
            if exists.is_none() {
                return Err(ClipForgeError::NotFound(format!("clip not found: {id}")));
            }
            conn.execute(
                "INSERT OR IGNORE INTO clip_tags (clip_id, tag) VALUES (?1, ?2)",
                params![id_str, tag_owned],
            )
            .map_err(db_err)?;
            Ok(())
        })
        .await
    }

    async fn remove_tag(&self, id: ClipId, tag: &str) -> Result<()> {
        let id_str = id.to_string();
        let tag_owned = tag.trim().to_string();
        self.with_conn(move |conn| {
            conn.execute(
                "DELETE FROM clip_tags WHERE clip_id = ?1 AND tag = ?2",
                params![id_str, tag_owned],
            )
            .map_err(db_err)?;
            Ok(())
        })
        .await
    }
}

const SELECT_CLIP_COLUMNS: &str = "
    SELECT
        c.id,
        c.title,
        c.game_id,
        c.game_name,
        c.game_executable,
        c.game_process_name,
        c.file_path,
        c.thumbnail_path,
        c.created_at,
        c.duration_ms,
        c.width,
        c.height,
        c.fps,
        c.size_bytes,
        c.codec,
        c.audio_tracks_json,
        c.cloud_share_json,
        c.notes
    FROM clips c";

fn configure_connection(conn: &Connection, enable_wal: bool) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(db_err)?;
    if enable_wal {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(db_err)?;
    }
    Ok(())
}

fn upsert_clip_sync(
    conn: &mut Connection,
    clip: &ClipMetadata,
    origin: Option<ClipOrigin>,
) -> Result<()> {
    let id_str = clip.id.to_string();
    let file_path_str = clip.file_path.to_string_lossy().into_owned();
    let thumb_path_str = clip
        .thumbnail_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    let created_at_str = clip.created_at.to_rfc3339();
    let audio_json = serde_json::to_string(&clip.audio_tracks)
        .map_err(|e| ClipForgeError::Serialization(e.to_string()))?;
    let cloud_json = clip
        .cloud_share
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| ClipForgeError::Serialization(e.to_string()))?;

    let (game_id, game_name, game_exe, game_proc) = match &clip.game {
        Some(g) => (
            Some(g.id.as_str()),
            Some(g.name.as_str()),
            g.executable.as_deref(),
            g.process_name.as_deref(),
        ),
        None => (None, None, None, None),
    };

    let insert_origin = origin.unwrap_or_default().as_str();
    let tx = conn.transaction().map_err(db_err)?;
    tx.execute(
        "INSERT INTO clips (
            id, title, game_id, game_name, game_executable, game_process_name,
            file_path, thumbnail_path, created_at, duration_ms, width, height,
            fps, size_bytes, codec, audio_tracks_json, cloud_share_json, notes, origin
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18, ?19
        )
        ON CONFLICT(id) DO UPDATE SET
            title = excluded.title,
            game_id = excluded.game_id,
            game_name = excluded.game_name,
            game_executable = excluded.game_executable,
            game_process_name = excluded.game_process_name,
            file_path = excluded.file_path,
            thumbnail_path = excluded.thumbnail_path,
            created_at = excluded.created_at,
            duration_ms = excluded.duration_ms,
            width = excluded.width,
            height = excluded.height,
            fps = excluded.fps,
            size_bytes = excluded.size_bytes,
            codec = excluded.codec,
            audio_tracks_json = excluded.audio_tracks_json,
            cloud_share_json = excluded.cloud_share_json,
            notes = excluded.notes,
            origin = CASE WHEN ?20 THEN excluded.origin ELSE clips.origin END",
        params![
            id_str,
            clip.title,
            game_id,
            game_name,
            game_exe,
            game_proc,
            file_path_str,
            thumb_path_str,
            created_at_str,
            clip.duration_ms as i64,
            i64::from(clip.width),
            i64::from(clip.height),
            f64::from(clip.fps),
            clip.size_bytes as i64,
            clip.codec,
            audio_json,
            cloud_json,
            clip.notes,
            insert_origin,
            origin.is_some(),
        ],
    )
    .map_err(db_err)?;

    tx.execute("DELETE FROM clip_tags WHERE clip_id = ?1", params![id_str])
        .map_err(db_err)?;

    let mut seen_tags = HashSet::new();
    for tag in &clip.tags {
        let trimmed = tag.trim();
        if !trimmed.is_empty() && seen_tags.insert(trimmed) {
            tx.execute(
                "INSERT INTO clip_tags (clip_id, tag) VALUES (?1, ?2)",
                params![id_str, trimmed],
            )
            .map_err(db_err)?;
        }
    }

    tx.commit().map_err(db_err)?;
    Ok(())
}

fn get_clip_sync(conn: &Connection, id_str: &str) -> Result<Option<ClipMetadata>> {
    let mut stmt = conn
        .prepare(&format!("{SELECT_CLIP_COLUMNS} WHERE c.id = ?1"))
        .map_err(db_err)?;
    let clip = stmt
        .query_row(params![id_str], row_to_clip_base)
        .optional()
        .map_err(db_err)?;

    match clip {
        Some(mut c) => {
            c.tags = load_tags_for_clip(conn, id_str)?;
            Ok(Some(c))
        }
        None => Ok(None),
    }
}

fn list_clips_sync(
    conn: &Connection,
    filter: &ClipFilter,
    options: ClipListOptions,
) -> Result<Vec<ClipMetadata>> {
    let mut sql = String::from(SELECT_CLIP_COLUMNS);
    let mut conditions: Vec<String> = Vec::new();
    let mut values: Vec<Value> = Vec::new();

    if let Some(tag) = filter
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        sql.push_str(" INNER JOIN clip_tags t ON t.clip_id = c.id");
        conditions.push("t.tag = ?".into());
        values.push(Value::Text(tag.to_string()));
    }

    if let Some(game_id) = filter.game_id.as_deref().filter(|g| !g.is_empty()) {
        conditions.push("c.game_id = ?".into());
        values.push(Value::Text(game_id.to_string()));
    }

    if options.favorites_only {
        conditions.push("c.is_favorite = 1".into());
    }

    if let Some(origin) = options.origin {
        conditions.push("c.origin = ?".into());
        values.push(Value::Text(origin.as_str().to_string()));
    } else if options.exclude_samples {
        conditions.push("c.origin != ?".into());
        values.push(Value::Text(ClipOrigin::Sample.as_str().to_string()));
    }

    if let Some(query) = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    {
        let escaped = escape_like(query);
        let pattern = format!("%{escaped}%");
        conditions.push(
            "(c.title LIKE ? ESCAPE '\\' OR COALESCE(c.notes, '') LIKE ? ESCAPE '\\' OR COALESCE(c.game_name, '') LIKE ? ESCAPE '\\')".into(),
        );
        values.push(Value::Text(pattern.clone()));
        values.push(Value::Text(pattern.clone()));
        values.push(Value::Text(pattern));
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    sql.push_str(" ORDER BY ");
    sql.push_str(options.sort.order_by_clause());

    if let Some(limit) = filter.limit {
        sql.push_str(" LIMIT ?");
        values.push(Value::Integer(limit as i64));
        if let Some(offset) = filter.offset {
            sql.push_str(" OFFSET ?");
            values.push(Value::Integer(offset as i64));
        }
    } else if let Some(offset) = filter.offset {
        sql.push_str(" LIMIT -1 OFFSET ?");
        values.push(Value::Integer(offset as i64));
    }

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map(params_from_iter(values), row_to_clip_base)
        .map_err(db_err)?;

    let mut clips = Vec::new();
    for row in rows {
        let mut clip = row.map_err(db_err)?;
        clip.tags = load_tags_for_clip(conn, &clip.id.to_string())?;
        clips.push(clip);
    }

    Ok(clips)
}

fn load_tags_for_clip(conn: &Connection, id_str: &str) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT tag FROM clip_tags WHERE clip_id = ?1 ORDER BY tag ASC")
        .map_err(db_err)?;
    let rows = stmt
        .query_map(params![id_str], |row| row.get::<_, String>(0))
        .map_err(db_err)?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row.map_err(db_err)?);
    }
    Ok(tags)
}

fn row_to_clip_base(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipMetadata> {
    let id_str: String = row.get(0)?;
    let id = id_str.parse::<ClipId>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let title: String = row.get(1)?;
    let game_id: Option<String> = row.get(2)?;
    let game_name: Option<String> = row.get(3)?;
    let game_executable: Option<String> = row.get(4)?;
    let game_process_name: Option<String> = row.get(5)?;
    let game = match (game_id, game_name) {
        (Some(gid), Some(gname)) => Some(GameInfo {
            id: gid,
            name: gname,
            executable: game_executable,
            process_name: game_process_name,
        }),
        _ => None,
    };

    let file_path = PathBuf::from(row.get::<_, String>(6)?);
    let thumbnail_path = row.get::<_, Option<String>>(7)?.map(PathBuf::from);
    let created_at_str: String = row.get(8)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let duration_ms = row.get::<_, i64>(9)?.max(0) as u64;
    let width = row.get::<_, i64>(10)?.max(0) as u32;
    let height = row.get::<_, i64>(11)?.max(0) as u32;
    let fps = row.get::<_, f64>(12)? as f32;
    let size_bytes = row.get::<_, i64>(13)?.max(0) as u64;
    let codec: String = row.get(14)?;

    let audio_json: String = row.get(15)?;
    let audio_tracks: Vec<AudioTrackInfo> = serde_json::from_str(&audio_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let cloud_json: Option<String> = row.get(16)?;
    let cloud_share: Option<CloudShareInfo> = cloud_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let notes: Option<String> = row.get(17)?;

    Ok(ClipMetadata {
        id,
        title,
        game,
        file_path,
        thumbnail_path,
        created_at,
        duration_ms,
        width,
        height,
        fps,
        size_bytes,
        codec,
        audio_tracks,
        tags: Vec::new(),
        cloud_share,
        notes,
    })
}

fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn db_err(e: rusqlite::Error) -> ClipForgeError {
    ClipForgeError::Database(e.to_string())
}
