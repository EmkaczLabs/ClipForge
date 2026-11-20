use std::collections::HashSet;
use std::path::{Path, PathBuf};

use clipforge_common::{ClipForgeError, Result};

use crate::store::SqliteClipStore;

const DEFAULT_VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "webm", "flv", "mov", "avi", "m4v"];

/// Scans a recordings directory for video files not yet indexed in [`SqliteClipStore`].
#[derive(Debug, Clone)]
pub struct DirectoryScanner {
    extensions: HashSet<String>,
    recursive: bool,
}

impl Default for DirectoryScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectoryScanner {
    /// Creates a recursive scanner for standard video container extensions.
    pub fn new() -> Self {
        Self {
            extensions: DEFAULT_VIDEO_EXTENSIONS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            recursive: true,
        }
    }

    /// Configures whether subdirectories are scanned recursively.
    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Overrides the recognized video file extensions (case-insensitive, without leading dots).
    pub fn with_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.extensions = extensions
            .into_iter()
            .map(|s| s.as_ref().trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        self
    }

    /// Scans `dir` and returns paths to video files not yet present in `store`.
    pub async fn scan_new_recordings(
        &self,
        dir: &Path,
        store: &SqliteClipStore,
    ) -> Result<Vec<PathBuf>> {
        let meta = tokio::fs::metadata(dir).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ClipForgeError::NotFound(format!(
                    "recordings directory not found: {}",
                    dir.display()
                ))
            } else {
                ClipForgeError::Io(e)
            }
        })?;

        if !meta.is_dir() {
            return Err(ClipForgeError::InvalidInput(format!(
                "recordings path is not a directory: {}",
                dir.display()
            )));
        }

        let known_raw = store.known_file_paths().await?;
        let mut known_normalized = HashSet::with_capacity(known_raw.len() * 2);
        for p in known_raw {
            if let Ok(canon) = std::fs::canonicalize(&p) {
                known_normalized.insert(canon);
            }
            known_normalized.insert(p);
        }

        let dir_owned = dir.to_path_buf();
        let extensions = self.extensions.clone();
        let recursive = self.recursive;

        tokio::task::spawn_blocking(move || {
            let mut discovered = Vec::new();
            collect_videos(&dir_owned, recursive, &extensions, &mut discovered)?;

            let mut new_files: Vec<PathBuf> = discovered
                .into_iter()
                .filter(|path| {
                    if known_normalized.contains(path) {
                        return false;
                    }
                    if let Ok(canon) = std::fs::canonicalize(path)
                        && known_normalized.contains(&canon)
                    {
                        return false;
                    }
                    true
                })
                .collect();

            new_files.sort();
            Ok(new_files)
        })
        .await
        .map_err(|e| ClipForgeError::Other(format!("directory scan task panicked: {e}")))?
    }
}

fn collect_videos(
    dir: &Path,
    recursive: bool,
    extensions: &HashSet<String>,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Skip hidden files/directories and in-progress ClipForge temp files.
        if name_str.starts_with('.') {
            continue;
        }

        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if recursive {
                collect_videos(&path, true, extensions, out)?;
            }
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && extensions.contains(&ext.to_ascii_lowercase())
        {
            out.push(path);
        }
    }
    Ok(())
}
