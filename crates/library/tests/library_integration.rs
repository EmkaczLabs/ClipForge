use std::path::PathBuf;

use chrono::{Duration, Utc};
use clipforge_common::{
    AudioTrackInfo, ClipFilter, ClipForgeError, ClipId, ClipMetadata, ClipStore, GameInfo,
};
use clipforge_library::{
    ClipListOptions, ClipOrigin, ClipSort, DirectoryScanner, PermanentDeleteResult,
    SqliteClipStore, TRASH_DIR_NAME,
};
use uuid::Uuid;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create test temp dir");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn make_clip(
    title: &str,
    file_path: PathBuf,
    game: Option<GameInfo>,
    duration_ms: u64,
    size_bytes: u64,
    offset_secs: i64,
    tags: Vec<&str>,
) -> ClipMetadata {
    ClipMetadata {
        id: ClipId::new(),
        title: title.to_string(),
        game,
        file_path,
        thumbnail_path: None,
        created_at: Utc::now() + Duration::seconds(offset_secs),
        duration_ms,
        width: 1920,
        height: 1080,
        fps: 60.0,
        size_bytes,
        codec: "h264".to_string(),
        audio_tracks: vec![AudioTrackInfo {
            index: 1,
            codec: "opus".to_string(),
            channels: 2,
            sample_rate: 48_000,
            label: Some("Game".to_string()),
        }],
        tags: tags.into_iter().map(String::from).collect(),
        cloud_share: None,
        notes: Some(format!("Notes for {title}")),
    }
}

#[tokio::test]
async fn crud_tags_favorites_and_game_association() {
    let store = SqliteClipStore::open_in_memory().expect("open in-memory db");
    assert_eq!(store.schema_version().await.unwrap(), 3);

    let game = GameInfo {
        id: "cs2".into(),
        name: "Counter-Strike 2".into(),
        executable: Some("cs2".into()),
        process_name: Some("cs2".into()),
    };

    let mut clip = make_clip(
        "Ace on Mirage",
        PathBuf::from("/clips/ace.mp4"),
        Some(game.clone()),
        15_000,
        20_000_000,
        0,
        vec!["clutch", "ace", "clutch"],
    );

    store.upsert_clip(&clip).await.expect("insert clip");

    let fetched = store
        .get_clip(clip.id)
        .await
        .expect("get clip")
        .expect("clip exists");
    assert_eq!(fetched.title, "Ace on Mirage");
    assert_eq!(fetched.game.as_ref().unwrap().id, "cs2");
    assert_eq!(fetched.tags, vec!["ace", "clutch"]);
    assert_eq!(fetched.audio_tracks.len(), 1);

    // Lookup by file path
    let by_path = store
        .get_by_file_path(PathBuf::from("/clips/ace.mp4").as_path())
        .await
        .unwrap()
        .expect("found by path");
    assert_eq!(by_path.id, clip.id);

    // Update via upsert
    clip.title = "Ace on Mirage (Edited)".into();
    clip.tags = vec!["highlight".into()];
    store.upsert_clip(&clip).await.expect("update clip");
    let updated = store.get_clip(clip.id).await.unwrap().unwrap();
    assert_eq!(updated.title, "Ace on Mirage (Edited)");
    assert_eq!(updated.tags, vec!["highlight"]);

    // Add & remove tags
    store.add_tag(clip.id, "fps").await.unwrap();
    let after_add = store.get_clip(clip.id).await.unwrap().unwrap();
    assert_eq!(after_add.tags, vec!["fps", "highlight"]);

    store.remove_tag(clip.id, "highlight").await.unwrap();
    let after_remove = store.get_clip(clip.id).await.unwrap().unwrap();
    assert_eq!(after_remove.tags, vec!["fps"]);

    // Adding tag to nonexistent clip returns NotFound
    let err = store.add_tag(ClipId::new(), "nope").await.unwrap_err();
    assert!(matches!(err, ClipForgeError::NotFound(_)));

    // Favorites
    assert!(!store.is_favorite(clip.id).await.unwrap());
    assert!(store.set_favorite(clip.id, true).await.unwrap());
    assert!(store.is_favorite(clip.id).await.unwrap());

    // Game association update & clear
    let new_game = GameInfo {
        id: "val".into(),
        name: "Valorant".into(),
        executable: None,
        process_name: None,
    };
    assert!(store.set_game(clip.id, Some(&new_game)).await.unwrap());
    assert_eq!(
        store
            .get_clip(clip.id)
            .await
            .unwrap()
            .unwrap()
            .game
            .unwrap()
            .id,
        "val"
    );
    assert!(store.set_game(clip.id, None).await.unwrap());
    assert!(
        store
            .get_clip(clip.id)
            .await
            .unwrap()
            .unwrap()
            .game
            .is_none()
    );

    // Delete
    assert!(store.delete_clip(clip.id).await.unwrap());
    assert!(!store.delete_clip(clip.id).await.unwrap());
    assert!(store.get_clip(clip.id).await.unwrap().is_none());
}

#[tokio::test]
async fn filter_search_sort_and_pagination() {
    let store = SqliteClipStore::open_in_memory().unwrap();

    let cs2 = GameInfo {
        id: "cs2".into(),
        name: "Counter-Strike 2".into(),
        executable: None,
        process_name: None,
    };
    let dota = GameInfo {
        id: "dota2".into(),
        name: "Dota 2".into(),
        executable: None,
        process_name: None,
    };

    let c1 = make_clip(
        "100% Clutch",
        PathBuf::from("/clips/c1.mp4"),
        Some(cs2.clone()),
        10_000,
        5_000,
        10,
        vec!["clutch"],
    );
    let c2 = make_clip(
        "Rampage",
        PathBuf::from("/clips/c2.mp4"),
        Some(dota.clone()),
        30_000,
        15_000,
        20,
        vec!["teamfight"],
    );
    let c3 = make_clip(
        "AWP Flick",
        PathBuf::from("/clips/c3.mp4"),
        Some(cs2.clone()),
        5_000,
        2_000,
        30,
        vec!["clutch", "awp"],
    );

    store.upsert_clip(&c1).await.unwrap();
    store.upsert_clip(&c2).await.unwrap();
    store.upsert_clip(&c3).await.unwrap();
    store.set_favorite(c2.id, true).await.unwrap();

    // Default list_clips: CreatedDesc
    let all = store.list_clips(&ClipFilter::default()).await.unwrap();
    assert_eq!(
        all.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![c3.id, c2.id, c1.id]
    );

    // Filter by game_id
    let cs2_clips = store
        .list_clips(&ClipFilter {
            game_id: Some("cs2".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(cs2_clips.len(), 2);

    // Filter by tag
    let clutch_clips = store
        .list_clips(&ClipFilter {
            tag: Some("clutch".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(clutch_clips.len(), 2);

    // Search with LIKE metacharacter `%` safely escaped
    let pct_clips = store
        .list_clips(&ClipFilter {
            query: Some("100%".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(pct_clips.len(), 1);
    assert_eq!(pct_clips[0].id, c1.id);

    // SQL injection attempt in query is treated as literal string
    let sqli_clips = store
        .list_clips(&ClipFilter {
            query: Some("'; DROP TABLE clips; --".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(sqli_clips.is_empty());
    assert_eq!(
        store
            .list_clips(&ClipFilter::default())
            .await
            .unwrap()
            .len(),
        3
    );

    // Favorites only + sorting
    let favs = store
        .list_clips_with_options(&ClipFilter::default(), ClipSort::CreatedAsc, true)
        .await
        .unwrap();
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0].id, c2.id);

    // Sort by DurationDesc
    let by_dur = store
        .list_clips_with_options(&ClipFilter::default(), ClipSort::DurationDesc, false)
        .await
        .unwrap();
    assert_eq!(
        by_dur.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![c2.id, c1.id, c3.id]
    );

    // Pagination (limit + offset)
    let paged = store
        .list_clips_with_options(
            &ClipFilter {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            },
            ClipSort::TitleAsc,
            false,
        )
        .await
        .unwrap();
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0].id, c3.id); // "100% Clutch", "AWP Flick", "Rampage"
}

#[tokio::test]
async fn file_backed_db_and_directory_scanner() {
    let dir = TestDir::new("clipforge_lib_scan");
    let db_path = dir.join("db/clipforge.sqlite");
    let recordings_dir = dir.join("recordings");
    let nested_dir = recordings_dir.join("2026-11");
    std::fs::create_dir_all(&nested_dir).unwrap();

    // Create sample files
    let known_file = recordings_dir.join("known.mp4");
    let new_mp4 = recordings_dir.join("new_clip.mp4");
    let new_mkv = nested_dir.join("nested_clip.mkv");
    let ignored_txt = recordings_dir.join("notes.txt");
    let ignored_tmp = recordings_dir.join(".clipforge_tmp_123.mp4");

    std::fs::write(&known_file, b"mp4").unwrap();
    std::fs::write(&new_mp4, b"mp4").unwrap();
    std::fs::write(&new_mkv, b"mkv").unwrap();
    std::fs::write(&ignored_txt, b"txt").unwrap();
    std::fs::write(&ignored_tmp, b"tmp").unwrap();

    // Open file-backed store, insert known_file, reopen to verify persistence & idempotent migrations
    {
        let store = SqliteClipStore::open(&db_path).expect("open file db");
        let clip = make_clip("Known", known_file.clone(), None, 1000, 3, 0, vec![]);
        store.upsert_clip(&clip).await.unwrap();
    }

    let store = SqliteClipStore::open(&db_path).expect("reopen file db");
    assert_eq!(store.schema_version().await.unwrap(), 3);

    let scanner = DirectoryScanner::new();
    let discovered = scanner
        .scan_new_recordings(&recordings_dir, &store)
        .await
        .expect("scan recursive");
    assert_eq!(discovered, vec![new_mkv.clone(), new_mp4.clone()]);

    // Non-recursive scan skips nested_dir
    let shallow = DirectoryScanner::new()
        .with_recursive(false)
        .scan_new_recordings(&recordings_dir, &store)
        .await
        .expect("scan shallow");
    assert_eq!(shallow, vec![new_mp4]);
}

#[tokio::test]
async fn clip_origin_distinguishes_samples_from_recordings_and_canonical_lookup() {
    let dir = TestDir::new("clipforge_lib_origin");
    let rec_file = dir.join("rec.mp4");
    std::fs::write(&rec_file, b"mp4").unwrap();
    let canon_rec = std::fs::canonicalize(&rec_file).unwrap();

    let store = SqliteClipStore::open_in_memory().unwrap();

    let recorded = make_clip(
        "Real Recording",
        canon_rec.clone(),
        None,
        10_000,
        1_000_000,
        10,
        vec![],
    );
    let sample = make_clip(
        "Sample Demo Clip",
        PathBuf::from("clips/sample-demo.mp4"),
        None,
        42_000,
        46_200_000,
        20,
        vec![],
    );
    let trimmed = make_clip(
        "Trimmed Export",
        dir.join("trimmed.mp4"),
        None,
        5_000,
        500_000,
        30,
        vec![],
    );

    // Default upsert_clip assigns Recorded origin
    store.upsert_clip(&recorded).await.unwrap();
    assert_eq!(
        store.get_origin(recorded.id).await.unwrap(),
        ClipOrigin::Recorded
    );

    store
        .upsert_clip_with_origin(&sample, ClipOrigin::Sample)
        .await
        .unwrap();
    store
        .upsert_clip_with_origin(&trimmed, ClipOrigin::Trimmed)
        .await
        .unwrap();

    assert!(store.get_origin(sample.id).await.unwrap().is_sample());
    assert!(!store.get_origin(trimmed.id).await.unwrap().is_sample());

    // Plain upsert_clip preserves existing origin
    let mut updated_sample = sample.clone();
    updated_sample.title = "Sample Demo Clip (Renamed)".into();
    store.upsert_clip(&updated_sample).await.unwrap();
    assert_eq!(
        store.get_origin(sample.id).await.unwrap(),
        ClipOrigin::Sample
    );

    // Filter excluding samples
    let non_samples = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                exclude_samples: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        non_samples.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![trimmed.id, recorded.id]
    );

    // Filter by exact origin
    let only_samples = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                origin: Some(ClipOrigin::Sample),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(only_samples.len(), 1);
    assert_eq!(only_samples[0].id, sample.id);

    // list_origins returns all 3
    let origins = store.list_origins().await.unwrap();
    assert_eq!(origins.get(&recorded.id), Some(&ClipOrigin::Recorded));
    assert_eq!(origins.get(&sample.id), Some(&ClipOrigin::Sample));
    assert_eq!(origins.get(&trimmed.id), Some(&ClipOrigin::Trimmed));

    // set_origin updates origin
    assert!(
        store
            .set_origin(recorded.id, ClipOrigin::Imported)
            .await
            .unwrap()
    );
    assert_eq!(
        store.get_origin(recorded.id).await.unwrap(),
        ClipOrigin::Imported
    );

    // get_by_file_path resolves non-canonical path with `.` segment
    let non_canon = dir.path.join(".").join("rec.mp4");
    let found = store
        .get_by_file_path(&non_canon)
        .await
        .unwrap()
        .expect("canonical fallback finds clip");
    assert_eq!(found.id, recorded.id);
}

#[tokio::test]
async fn trash_restore_and_scanner_skips_trash_dir() {
    let dir = TestDir::new("clipforge_trash_restore");
    let store = SqliteClipStore::open_in_memory().unwrap();
    let roots = vec![dir.path.clone()];

    let rec_path = dir.join("clip_a.mp4");
    std::fs::write(&rec_path, b"video-a").unwrap();
    let rec = make_clip("Clip A", rec_path.clone(), None, 5_000, 1_000, 0, vec![]);
    store
        .upsert_clip_with_origin(&rec, ClipOrigin::Recorded)
        .await
        .unwrap();

    // Move to trash
    let moved = store.move_to_trash(rec.id, &roots).await.unwrap();
    assert!(moved.file_moved);
    assert_eq!(moved.original_path, rec_path);
    assert!(!rec_path.exists());
    assert!(moved.clip.file_path.exists());
    assert!(
        moved
            .clip
            .file_path
            .starts_with(dir.path.canonicalize().unwrap().join(TRASH_DIR_NAME))
    );

    // Trash info is recorded in SQLite
    let info = store
        .get_trash_info(rec.id)
        .await
        .unwrap()
        .expect("trash info present");
    assert_eq!(info.original_path, rec_path);

    // Default listing excludes trashed; trashed_only includes it
    assert!(
        store
            .list_clips(&ClipFilter::default())
            .await
            .unwrap()
            .is_empty()
    );
    let trashed = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                trashed_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].id, rec.id);

    // DirectoryScanner must skip .clipforge_trash
    let discovered = DirectoryScanner::new()
        .scan_new_recordings(&dir.path, &store)
        .await
        .unwrap();
    assert!(discovered.is_empty());

    // Cannot move an already-trashed clip again
    assert!(matches!(
        store.move_to_trash(rec.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));

    // Restore back to original path
    let restored = store.restore_from_trash(rec.id).await.unwrap();
    assert!(restored.file_restored);
    assert_eq!(restored.restored_path, rec_path);
    assert!(rec_path.exists());
    assert!(!moved.clip.file_path.exists());
    assert!(store.get_trash_info(rec.id).await.unwrap().is_none());

    // Normal listing sees it again
    let active = store.list_clips(&ClipFilter::default()).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].file_path, rec_path);

    // Restore collision: trash again, place a new file at rec_path, then restore
    let moved2 = store.move_to_trash(rec.id, &roots).await.unwrap();
    assert!(moved2.file_moved);
    std::fs::write(&rec_path, b"new-file-at-original").unwrap();

    let restored2 = store.restore_from_trash(rec.id).await.unwrap();
    assert_ne!(restored2.restored_path, rec_path);
    assert!(restored2.restored_path.exists());
    assert_eq!(std::fs::read(&restored2.restored_path).unwrap(), b"video-a");
    assert_eq!(std::fs::read(&rec_path).unwrap(), b"new-file-at-original");
}

#[tokio::test]
async fn trash_and_delete_safety_boundaries_and_missing_files() {
    let managed = TestDir::new("clipforge_managed");
    let external = TestDir::new("clipforge_external");
    let store = SqliteClipStore::open_in_memory().unwrap();
    let roots = vec![managed.path.clone()];

    // Sample and Imported clips must NEVER be moved to trash or deleted on disk
    let sample_path = managed.join("sample.mp4");
    std::fs::write(&sample_path, b"sample-bytes").unwrap();
    let sample = make_clip("Sample", sample_path.clone(), None, 1_000, 100, 0, vec![]);
    store
        .upsert_clip_with_origin(&sample, ClipOrigin::Sample)
        .await
        .unwrap();

    let imported_path = external.join("imported.mp4");
    std::fs::write(&imported_path, b"imported-bytes").unwrap();
    let imported = make_clip(
        "Imported",
        imported_path.clone(),
        None,
        1_000,
        100,
        1,
        vec![],
    );
    store
        .upsert_clip_with_origin(&imported, ClipOrigin::Imported)
        .await
        .unwrap();

    assert!(matches!(
        store.move_to_trash(sample.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));
    assert!(matches!(
        store.move_to_trash(imported.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));
    assert!(sample_path.exists());
    assert!(imported_path.exists());

    // permanently_delete on Sample/Imported removes DB row only, never disk file
    let del_sample = store.permanently_delete(sample.id, &roots).await.unwrap();
    assert_eq!(
        del_sample,
        PermanentDeleteResult {
            deleted_from_db: true,
            file_deleted: false
        }
    );
    assert!(sample_path.exists());
    assert!(store.get_clip(sample.id).await.unwrap().is_none());

    let del_imported = store.permanently_delete(imported.id, &roots).await.unwrap();
    assert_eq!(
        del_imported,
        PermanentDeleteResult {
            deleted_from_db: true,
            file_deleted: false
        }
    );
    assert!(imported_path.exists());

    // Recorded clip outside managed_roots (existing or missing) must be rejected
    let outside_path = external.join("outside.mp4");
    std::fs::write(&outside_path, b"outside").unwrap();
    let outside = make_clip("Outside", outside_path.clone(), None, 1_000, 100, 2, vec![]);
    store
        .upsert_clip_with_origin(&outside, ClipOrigin::Recorded)
        .await
        .unwrap();

    assert!(matches!(
        store.move_to_trash(outside.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));
    assert!(matches!(
        store.permanently_delete(outside.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));
    assert!(outside_path.exists());
    assert!(store.get_clip(outside.id).await.unwrap().is_some());

    // Missing out-of-bounds path with traversal must also be rejected
    let traversal = make_clip(
        "Traversal",
        managed.path.join("..").join("escape.mp4"),
        None,
        1_000,
        100,
        3,
        vec![],
    );
    store
        .upsert_clip_with_origin(&traversal, ClipOrigin::Recorded)
        .await
        .unwrap();
    assert!(matches!(
        store.move_to_trash(traversal.id, &roots).await,
        Err(ClipForgeError::InvalidInput(_))
    ));

    // Symlink inside managed_roots pointing outside managed_roots must be rejected
    #[cfg(unix)]
    {
        let link_path = managed.join("symlink_escape.mp4");
        std::os::unix::fs::symlink(&outside_path, &link_path).unwrap();
        let sym_clip = make_clip("Symlink", link_path.clone(), None, 1_000, 100, 4, vec![]);
        store
            .upsert_clip_with_origin(&sym_clip, ClipOrigin::Recorded)
            .await
            .unwrap();
        assert!(matches!(
            store.move_to_trash(sym_clip.id, &roots).await,
            Err(ClipForgeError::InvalidInput(_))
        ));
        assert!(outside_path.exists());
    }

    // Missing file inside managed_roots: move_to_trash marks trashed with file_moved = false
    let missing_path = managed.join("already_gone.mp4");
    let missing = make_clip("Missing", missing_path.clone(), None, 1_000, 100, 5, vec![]);
    store
        .upsert_clip_with_origin(&missing, ClipOrigin::Trimmed)
        .await
        .unwrap();

    let missing_trashed = store.move_to_trash(missing.id, &roots).await.unwrap();
    assert!(!missing_trashed.file_moved);
    assert!(store.get_trash_info(missing.id).await.unwrap().is_some());

    // Restoring when trash file is missing returns NotFound and preserves trash state
    assert!(matches!(
        store.restore_from_trash(missing.id).await,
        Err(ClipForgeError::NotFound(_))
    ));
    assert!(store.get_trash_info(missing.id).await.unwrap().is_some());

    // Permanently deleting missing clip cleans up DB row gracefully
    let del_missing = store.permanently_delete(missing.id, &roots).await.unwrap();
    assert_eq!(
        del_missing,
        PermanentDeleteResult {
            deleted_from_db: true,
            file_deleted: false
        }
    );
    assert!(store.get_clip(missing.id).await.unwrap().is_none());

    // Permanently deleting a trashed Recorded clip removes both trash file and DB row
    let real_path = managed.join("real_delete.mp4");
    std::fs::write(&real_path, b"real").unwrap();
    let real = make_clip("Real", real_path.clone(), None, 1_000, 100, 6, vec![]);
    store
        .upsert_clip_with_origin(&real, ClipOrigin::Recorded)
        .await
        .unwrap();
    let trashed_real = store.move_to_trash(real.id, &roots).await.unwrap();
    assert!(trashed_real.clip.file_path.exists());

    let del_real = store.permanently_delete(real.id, &roots).await.unwrap();
    assert_eq!(
        del_real,
        PermanentDeleteResult {
            deleted_from_db: true,
            file_deleted: true
        }
    );
    assert!(!trashed_real.clip.file_path.exists());
    assert!(store.get_clip(real.id).await.unwrap().is_none());

    // Deleting non-existent ID returns false/false
    assert_eq!(
        store
            .permanently_delete(ClipId::new(), &roots)
            .await
            .unwrap(),
        PermanentDeleteResult {
            deleted_from_db: false,
            file_deleted: false
        }
    );
}

#[tokio::test]
async fn rename_clip_and_trash_filtering_with_sorting() {
    let dir = TestDir::new("clipforge_rename_sort");
    let store = SqliteClipStore::open_in_memory().unwrap();
    let roots = vec![dir.path.clone()];

    let p1 = dir.join("c1.mp4");
    let p2 = dir.join("c2.mp4");
    let p3 = dir.join("c3.mp4");
    std::fs::write(&p1, b"1").unwrap();
    std::fs::write(&p2, b"2").unwrap();
    std::fs::write(&p3, b"3").unwrap();

    let c1 = make_clip("Zebra", p1, None, 10_000, 500, 10, vec![]);
    let c2 = make_clip("Alpha", p2, None, 30_000, 1_500, 20, vec![]);
    let c3 = make_clip("Middle", p3, None, 20_000, 1_000, 30, vec![]);
    store.upsert_clip(&c1).await.unwrap();
    store.upsert_clip(&c2).await.unwrap();
    store.upsert_clip(&c3).await.unwrap();

    // Rename trims whitespace and rejects empty
    assert!(store.rename_clip(c1.id, "  Beta  ").await.unwrap());
    assert_eq!(store.get_clip(c1.id).await.unwrap().unwrap().title, "Beta");
    assert!(!store.rename_clip(ClipId::new(), "Valid").await.unwrap());
    assert!(matches!(
        store.rename_clip(c1.id, "   ").await,
        Err(ClipForgeError::InvalidInput(_))
    ));

    // Trash c3
    store.move_to_trash(c3.id, &roots).await.unwrap();

    // Active clips sorted by TitleAsc -> Alpha, Beta
    let active_by_title = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                sort: ClipSort::TitleAsc,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        active_by_title.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![c2.id, c1.id]
    );

    // Active clips sorted by SizeDesc -> Alpha (1500), Beta (500)
    let active_by_size = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                sort: ClipSort::SizeDesc,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        active_by_size.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![c2.id, c1.id]
    );

    // Include trashed (exclude_trashed: false) -> all 3
    let all_clips = store
        .list_clips_filtered(
            &ClipFilter::default(),
            ClipListOptions {
                sort: ClipSort::CreatedAsc,
                exclude_trashed: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        all_clips.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![c1.id, c2.id, c3.id]
    );
}
