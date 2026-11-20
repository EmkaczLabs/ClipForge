use std::path::PathBuf;

use chrono::{Duration, Utc};
use clipforge_common::{
    AudioTrackInfo, ClipFilter, ClipForgeError, ClipId, ClipMetadata, ClipStore, GameInfo,
};
use clipforge_library::{ClipSort, DirectoryScanner, SqliteClipStore};
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
    assert_eq!(store.schema_version().await.unwrap(), 1);

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
    assert_eq!(store.schema_version().await.unwrap(), 1);

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
