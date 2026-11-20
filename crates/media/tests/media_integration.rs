use std::path::{Path, PathBuf};
use std::process::Command;

use clipforge_common::{ClipForgeError, MediaProcessor, TrimRequest, VideoCodec};
use clipforge_media::FfmpegMediaProcessor;
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

fn generate_synthetic_mp4(dest: &Path) {
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=3:size=320x240:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(dest)
        .status()
        .expect("run ffmpeg to generate synthetic mp4");
    assert!(status.success(), "ffmpeg synthetic generation failed");
}

#[tokio::test]
async fn probe_thumbnail_and_trim_synthetic_mp4() {
    let dir = TestDir::new("clipforge_media_it");
    let source = dir.join("synthetic.mp4");
    generate_synthetic_mp4(&source);

    let processor = FfmpegMediaProcessor::with_concurrency(2);

    // 1. Probe
    let probe = processor.probe(&source).await.expect("probe synthetic mp4");
    assert_eq!(probe.width, 320);
    assert_eq!(probe.height, 240);
    assert!(
        (probe.fps - 30.0).abs() < 1.0,
        "unexpected fps: {}",
        probe.fps
    );
    assert!(
        (2800..=3200).contains(&probe.duration_ms),
        "unexpected duration_ms: {}",
        probe.duration_ms
    );
    assert_eq!(probe.video_codec, "h264");
    assert!(probe.size_bytes > 0);
    assert_eq!(probe.audio_tracks.len(), 1);
    assert_eq!(probe.audio_tracks[0].codec, "aac");

    // 2. Thumbnails (PNG + JPEG, concurrent)
    let thumb_png = dir.join("thumbs/frame.png");
    let thumb_jpg = dir.join("thumbs/frame.jpg");
    let (res1, res2) = tokio::join!(
        processor.generate_thumbnail(&source, 1000, &thumb_png),
        processor.generate_thumbnail(&source, 1500, &thumb_jpg),
    );
    res1.expect("generate png thumbnail");
    res2.expect("generate jpg thumbnail");

    let png_bytes = std::fs::read(&thumb_png).expect("read png");
    assert!(
        png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "invalid PNG magic"
    );

    let jpg_bytes = std::fs::read(&thumb_jpg).expect("read jpg");
    assert!(
        jpg_bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "invalid JPEG magic"
    );

    // 3. Trim (re-encode for exact 1500ms duration)
    let trimmed_reencode = dir.join("exports/trimmed_reencode.mp4");
    let out_path = processor
        .trim(&TrimRequest {
            source_path: source.clone(),
            output_path: trimmed_reencode.clone(),
            start_ms: 500,
            end_ms: 2000,
            reencode: true,
            video_codec: Some(VideoCodec::H264),
        })
        .await
        .expect("trim with reencode");
    assert_eq!(out_path, trimmed_reencode);

    let trimmed_probe = processor
        .probe(&trimmed_reencode)
        .await
        .expect("probe trimmed reencoded");
    assert!(
        (1200..=1800).contains(&trimmed_probe.duration_ms),
        "unexpected trimmed duration_ms: {}",
        trimmed_probe.duration_ms
    );
    assert_eq!(trimmed_probe.audio_tracks.len(), 1);

    // 4. Trim (stream copy)
    let trimmed_copy = dir.join("exports/trimmed_copy.mp4");
    processor
        .trim(&TrimRequest {
            source_path: source.clone(),
            output_path: trimmed_copy.clone(),
            start_ms: 0,
            end_ms: 2000,
            reencode: false,
            video_codec: None,
        })
        .await
        .expect("trim with stream copy");
    let copy_probe = processor
        .probe(&trimmed_copy)
        .await
        .expect("probe trimmed copy");
    assert!(copy_probe.duration_ms > 0);
    assert_eq!(copy_probe.audio_tracks.len(), 1);

    // Verify no leftover temp files in exports/
    let leftover_tmps: Vec<_> = std::fs::read_dir(dir.join("exports"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".clipforge_tmp_"))
        .collect();
    assert!(
        leftover_tmps.is_empty(),
        "leftover temp files found: {leftover_tmps:?}"
    );
}

#[tokio::test]
async fn validation_rejects_bad_paths_and_ranges() {
    let dir = TestDir::new("clipforge_media_val");
    let processor = FfmpegMediaProcessor::new();

    // Missing input file
    let err = processor.probe(&dir.join("missing.mp4")).await.unwrap_err();
    assert!(matches!(err, ClipForgeError::NotFound(_)));

    // Directory instead of file
    let err = processor.probe(&dir.path).await.unwrap_err();
    assert!(matches!(err, ClipForgeError::InvalidInput(_)));

    // Null byte in path
    let err = processor
        .probe(Path::new("bad\0path.mp4"))
        .await
        .unwrap_err();
    assert!(matches!(err, ClipForgeError::InvalidInput(_)));

    // Invalid trim range (start >= end)
    let err = processor
        .trim(&TrimRequest {
            source_path: dir.join("a.mp4"),
            output_path: dir.join("b.mp4"),
            start_ms: 2000,
            end_ms: 1000,
            reencode: false,
            video_codec: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, ClipForgeError::InvalidInput(_)));

    // Invalid thumbnail extension
    let dummy_vid = dir.join("dummy.mp4");
    std::fs::write(&dummy_vid, b"not a video").unwrap();
    let err = processor
        .generate_thumbnail(&dummy_vid, 0, &dir.join("thumb.bmp"))
        .await
        .unwrap_err();
    assert!(matches!(err, ClipForgeError::InvalidInput(_)));
}

#[tokio::test]
async fn gsr_like_mkv_multi_audio_and_past_eof_thumbnail_fallback() {
    let dir = TestDir::new("clipforge_media_mkv");
    let mkv_path = dir.join("gsr_replay.mkv");

    // Generate a GSR-like MKV: H.264 60fps + 2 Opus audio tracks (Game + Mic)
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=2:size=640x360:rate=60",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=2",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:a:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "libopus",
            "-metadata:s:a:0",
            "title=Desktop Audio",
            "-metadata:s:a:1",
            "title=Microphone",
        ])
        .arg(&mkv_path)
        .status()
        .expect("run ffmpeg to generate gsr-like mkv");
    assert!(status.success(), "ffmpeg mkv generation failed");

    let processor = FfmpegMediaProcessor::new();
    let probe = processor.probe(&mkv_path).await.expect("probe mkv");
    assert_eq!(probe.width, 640);
    assert_eq!(probe.height, 360);
    assert!((probe.fps - 60.0).abs() < 1.0);
    assert!((1800..=2200).contains(&probe.duration_ms));
    assert_eq!(probe.video_codec, "h264");
    assert_eq!(probe.audio_tracks.len(), 2);
    assert_eq!(probe.audio_tracks[0].codec, "opus");
    assert_eq!(
        probe.audio_tracks[0].label.as_deref(),
        Some("Desktop Audio")
    );
    assert_eq!(probe.audio_tracks[1].label.as_deref(), Some("Microphone"));

    // Past-EOF timestamp (60s on a 2s clip) should fall back to 0ms and produce a valid PNG
    let thumb_path = dir.join("thumbs/past_eof.png");
    processor
        .generate_thumbnail(&mkv_path, 60_000, &thumb_path)
        .await
        .expect("thumbnail with past-eof fallback");
    let png_bytes = std::fs::read(&thumb_path).expect("read fallback png");
    assert!(png_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[tokio::test]
async fn video_player_open_seek_play_trim_and_switch_clips() {
    use clipforge_media::{PlaybackState, PlayerEvent, VideoPlayerHandle};
    use std::time::Duration;

    let dir = TestDir::new("clipforge_player_it");
    let clip_a = dir.join("clip_a.mp4");
    let clip_b = dir.join("clip_b.mp4");
    generate_synthetic_mp4(&clip_a);
    generate_synthetic_mp4(&clip_b);

    let (player, mut rx) = VideoPlayerHandle::spawn();

    // Helper to collect events until a predicate matches or timeout expires.
    async fn wait_for<F>(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>,
        mut pred: F,
    ) -> Vec<PlayerEvent>
    where
        F: FnMut(&PlayerEvent) -> bool,
    {
        let mut collected = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for player event; collected={collected:?}"
            );
            let ev = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("timeout receiving event")
                .expect("event channel closed");
            let done = pred(&ev);
            collected.push(ev);
            if done {
                return collected;
            }
        }
    }

    // 1. Open clip_a with trim range [500, 1200] and max_dim = 160 (320x240 -> 160x120)
    player.open(&clip_a, Some((500, 1200)), Some(160));
    let events = wait_for(&mut rx, |e| {
        matches!(e, PlayerEvent::StateChanged(PlaybackState::Paused))
    })
    .await;

    let meta = events
        .iter()
        .find_map(|e| match e {
            PlayerEvent::Metadata(m) => Some(m.clone()),
            _ => None,
        })
        .expect("expected Metadata event");
    assert_eq!(meta.width, 320);
    assert_eq!(meta.height, 240);
    assert!((meta.aspect_ratio - (320.0 / 240.0)).abs() < 0.01);
    assert!(meta.has_audio);

    let first_frame = events
        .iter()
        .find_map(|e| match e {
            PlayerEvent::Frame(f) => Some(f.clone()),
            _ => None,
        })
        .expect("expected initial Frame event");
    assert_eq!(first_frame.width, 160);
    assert_eq!(first_frame.height, 120);
    assert_eq!(first_frame.data.len(), 160 * 120 * 4);
    assert_eq!(first_frame.pts_ms, 500);

    // 2. Seek outside trim range clamps to [500, 1200]
    player.seek(100);
    let seek_low = wait_for(&mut rx, |e| matches!(e, PlayerEvent::Position(_))).await;
    assert!(
        seek_low
            .iter()
            .any(|e| matches!(e, PlayerEvent::Position(500)))
    );

    player.seek(2500);
    let seek_high = wait_for(&mut rx, |e| matches!(e, PlayerEvent::Position(_))).await;
    assert!(
        seek_high
            .iter()
            .any(|e| matches!(e, PlayerEvent::Position(1200)))
    );

    // 3. Play from trim start (since position was at trim end 1200, play() restarts from 500)
    player.play();
    let play_events = wait_for(&mut rx, |e| {
        matches!(e, PlayerEvent::StateChanged(PlaybackState::Ended))
    })
    .await;
    let decoded_frames: Vec<_> = play_events
        .iter()
        .filter_map(|e| match e {
            PlayerEvent::Frame(f) => Some(f),
            _ => None,
        })
        .collect();
    assert!(
        decoded_frames.len() >= 5,
        "expected multiple decoded frames during playback, got {}",
        decoded_frames.len()
    );
    assert!(
        decoded_frames
            .iter()
            .all(|f| f.pts_ms >= 500 && f.pts_ms <= 1200 && f.data.len() == 160 * 120 * 4)
    );

    // 4. Restart and pause mid-playback, then switch to clip_b cleanly
    player.restart();
    let _ = wait_for(&mut rx, |e| {
        matches!(e, PlayerEvent::StateChanged(PlaybackState::Playing))
    })
    .await;
    player.open(&clip_b, None, Some(1024));
    let switch_events = wait_for(&mut rx, |e| {
        matches!(e, PlayerEvent::StateChanged(PlaybackState::Paused))
    })
    .await;
    let b_frame = switch_events
        .iter()
        .find_map(|e| match e {
            PlayerEvent::Frame(f) => Some(f.clone()),
            _ => None,
        })
        .expect("expected frame from clip_b");
    assert_eq!(b_frame.width, 320);
    assert_eq!(b_frame.height, 240);

    // 5. Close releases clip and transitions to Stopped
    player.close();
    let _ = wait_for(&mut rx, |e| {
        matches!(e, PlayerEvent::StateChanged(PlaybackState::Stopped))
    })
    .await;
}

#[tokio::test]
async fn video_player_handles_missing_and_corrupt_files_gracefully() {
    use clipforge_media::{PlaybackState, PlayerEvent, VideoPlayerHandle};
    use std::time::Duration;

    let dir = TestDir::new("clipforge_player_err");
    let missing = dir.join("does_not_exist.mp4");
    let corrupt = dir.join("corrupt.mp4");
    std::fs::write(&corrupt, b"not a valid mp4 file at all").unwrap();

    let (player, mut rx) = VideoPlayerHandle::spawn();

    for bad_path in [&missing, &corrupt] {
        player.open(bad_path, None, Some(1024));
        let mut saw_error = false;
        let mut saw_stopped = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !(saw_error && saw_stopped) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for error/stopped");
            let ev = tokio::time::timeout(remaining, rx.recv())
                .await
                .expect("timeout")
                .expect("closed");
            match ev {
                PlayerEvent::Error(_) => saw_error = true,
                PlayerEvent::StateChanged(PlaybackState::Stopped) => saw_stopped = true,
                _ => {}
            }
        }
    }
}

#[tokio::test]
async fn extract_timeline_thumbnails_returns_scaled_rgba_frames() {
    use clipforge_media::extract_timeline_thumbnails;

    let dir = TestDir::new("clipforge_thumbs_it");
    let source = dir.join("synthetic.mp4");
    generate_synthetic_mp4(&source);

    let thumbs = extract_timeline_thumbnails(&source, 5, 160, 90)
        .await
        .expect("extract timeline thumbnails");
    assert_eq!(thumbs.len(), 5);
    for thumb in &thumbs {
        // 320x240 fit within 160x90 -> 120x90
        assert_eq!(thumb.width, 120);
        assert_eq!(thumb.height, 90);
        assert_eq!(thumb.data.len(), 120 * 90 * 4);
    }

    let missing_err = extract_timeline_thumbnails(&dir.join("missing.mp4"), 3, 160, 90)
        .await
        .unwrap_err();
    assert!(matches!(missing_err, ClipForgeError::NotFound(_)));
}
