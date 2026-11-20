use std::process::Command;
use std::time::Duration;

use clipforge_capture::{GsrCaptureBackend, detect_gsr, list_monitors};
use clipforge_common::{CaptureBackend, CaptureConfig, CaptureTarget};

fn ffprobe_audio_streams(path: &std::path::Path) -> Vec<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=index,codec_name,channels:stream_tags=name",
            "-select_streams",
            "a",
            "-of",
            "compact=p=0:nk=0",
        ])
        .arg(path)
        .output()
        .expect("ffprobe must run");
    assert!(
        out.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect()
}

async fn run_case(
    monitor: &str,
    sources: Vec<String>,
    expected_streams: usize,
    expected_name_substrings: &[&str],
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = CaptureConfig {
        output_dir: dir.path().to_path_buf(),
        fps: 30,
        capture_target: CaptureTarget::Monitor(monitor.to_string()),
        audio_sources: sources.clone(),
        ..CaptureConfig::default()
    };

    let backend = GsrCaptureBackend::new();
    backend
        .start_recording(&cfg)
        .await
        .expect("start_recording");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    backend.stop().await.expect("stop");

    let output_path = std::fs::read_dir(dir.path())
        .expect("read_dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|ext| ext == "mp4"))
        .expect("recorded mp4 file");

    let streams = ffprobe_audio_streams(&output_path);
    println!("sources={sources:?} -> {} streams:", streams.len());
    for s in &streams {
        println!("  {s}");
    }
    assert_eq!(
        streams.len(),
        expected_streams,
        "unexpected audio stream count for {sources:?}"
    );
    for (idx, expected) in expected_name_substrings.iter().enumerate() {
        assert!(
            streams[idx].contains(expected),
            "stream {idx} ({:?}) did not contain {expected:?}",
            streams[idx]
        );
    }
}

#[tokio::test]
#[ignore = "requires live gpu-screen-recorder, PipeWire, and display session"]
async fn live_gsr_audio_routing_matrix() {
    let install = detect_gsr().await.expect("gsr installed");
    let monitors = list_monitors(&install).await.expect("monitors");
    let monitor = monitors.first().expect("at least one monitor").name.clone();

    // (1) System audio only
    run_case(
        &monitor,
        vec!["default_output".to_string()],
        1,
        &["Default output"],
    )
    .await;

    // (2) Mic only
    run_case(
        &monitor,
        vec!["default_input".to_string()],
        1,
        &["Default input"],
    )
    .await;

    // (3) System + Mic (Auto -> single mixed stream)
    run_case(
        &monitor,
        vec!["default_output".to_string(), "default_input".to_string()],
        1,
        &["Default output, Default input"],
    )
    .await;

    // (4) System + Mic (T2 -> 2 separate streams)
    run_case(
        &monitor,
        vec![
            "t1:default_output".to_string(),
            "t2:default_input".to_string(),
        ],
        2,
        &["Default output", "Default input"],
    )
    .await;

    // (5) System + Mic (T3/T4 -> 3 separate streams)
    run_case(
        &monitor,
        vec![
            "t1:default_output".to_string(),
            "t3:default_input".to_string(),
            "t4:easyeffects_source".to_string(),
        ],
        3,
        &["Default output", "Default input", "Easy Effects Source"],
    )
    .await;
}
