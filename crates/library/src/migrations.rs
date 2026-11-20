use clipforge_common::{ClipForgeError, Result};
use rusqlite::Connection;

const LATEST_VERSION: u32 = 1;

fn db_err(e: rusqlite::Error) -> ClipForgeError {
    ClipForgeError::Database(e.to_string())
}

/// Applies any pending SQLite schema migrations to `conn`.
pub fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
        );",
    )
    .map_err(db_err)?;

    let current = current_schema_version(conn)?;
    if current < 1 {
        apply_v1(conn)?;
    }

    let _ = LATEST_VERSION;
    Ok(())
}

/// Returns the highest applied schema migration version (0 if none).
pub fn current_schema_version(conn: &Connection) -> Result<u32> {
    let version: Option<u32> = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(db_err)?;
    Ok(version.unwrap_or(0))
}

fn apply_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "BEGIN IMMEDIATE;
        CREATE TABLE IF NOT EXISTS clips (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            game_id TEXT,
            game_name TEXT,
            game_executable TEXT,
            game_process_name TEXT,
            file_path TEXT NOT NULL UNIQUE,
            thumbnail_path TEXT,
            created_at TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL,
            fps REAL NOT NULL,
            size_bytes INTEGER NOT NULL,
            codec TEXT NOT NULL,
            audio_tracks_json TEXT NOT NULL,
            cloud_share_json TEXT,
            notes TEXT,
            is_favorite INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS clip_tags (
            clip_id TEXT NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
            tag TEXT NOT NULL,
            PRIMARY KEY (clip_id, tag)
        );

        CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_clips_game_id ON clips(game_id);
        CREATE INDEX IF NOT EXISTS idx_clips_favorite ON clips(is_favorite);
        CREATE INDEX IF NOT EXISTS idx_clip_tags_tag ON clip_tags(tag);

        INSERT INTO schema_migrations (version) VALUES (1);
        COMMIT;",
    )
    .map_err(db_err)?;

    Ok(())
}
