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
