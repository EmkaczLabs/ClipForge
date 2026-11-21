//! ClipForge Desktop — native GPU-rendered Linux game clipping app built with Rune.

#![forbid(unsafe_code)]

mod components;
mod config;
mod state;
mod theme;
mod views;
mod worker;

use std::time::Instant;

use clipforge_capture::{HotkeyAction, parse_hotkey};
use rune::{AppSpec, Key, KeyEvent, Tx, Waker};
use state::{AppState, HotkeyConfig, NavSection};
use worker::{WorkerCommand, WorkerHandle};

fn setup_worker(state: &mut AppState, waker: Waker) {
    let t0 = Instant::now();
    let handle = WorkerHandle::spawn(waker);
    handle.send(WorkerCommand::Initialize(state.config.clone()));
    state.worker = Some(handle);
    tracing::info!(
        elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0,
        "setup_worker spawned background worker"
    );
}

fn poll_worker(state: &mut AppState, _now: f64, _tx: &mut Tx) -> bool {
    state.poll_worker()
}

fn key_event_to_keysym(key: &Key) -> Option<u32> {
    match key {
        Key::Function(n) if (1..=24).contains(n) => Some(0xFFBE + u32::from(n - 1)),
        Key::Text(s) if s.chars().count() == 1 => {
            let ch = s.chars().next()?.to_ascii_lowercase();
            ch.is_ascii_alphanumeric().then_some(ch as u32)
        }
        Key::Text(s) if s == " " => Some(0x0020),
        Key::Enter => Some(0xFF0D),
        Key::Escape => Some(0xFF1B),
        Key::Tab => Some(0xFF09),
        Key::PrintScreen => Some(0xFF61),
        Key::Insert => Some(0xFF63),
        Key::Delete => Some(0xFFFF),
        Key::Home => Some(0xFF50),
        Key::End => Some(0xFF57),
        Key::Pause => Some(0xFF13),
        _ => None,
    }
}

fn match_key_event_to_hotkey(event: &KeyEvent, hotkeys: &HotkeyConfig) -> Option<HotkeyAction> {
    let keysym = key_event_to_keysym(&event.key)?;
    for (action, raw) in [
        (HotkeyAction::SaveReplay, hotkeys.save_replay.as_str()),
        (
            HotkeyAction::ToggleReplayBuffer,
            hotkeys.toggle_replay.as_str(),
        ),
        (
            HotkeyAction::ToggleRecording,
            hotkeys.toggle_record.as_str(),
        ),
    ] {
        if let Ok(spec) = parse_hotkey(raw)
            && spec.matches_parts(
                event.mods.ctrl,
                event.mods.alt,
                event.mods.shift,
                event.mods.logo,
                keysym,
            )
        {
            return Some(action);
        }
    }
    None
}

fn on_key(state: &mut AppState, event: &KeyEvent, _tx: &mut Tx) -> bool {
    let is_function_key = matches!(event.key, Key::Function(_));
    if (state.renaming_clip_id.is_none() || is_function_key)
        && let Some(action) = match_key_event_to_hotkey(event, &state.hotkeys)
    {
        state.dispatch_hotkey_action(action);
        return true;
    }

    if state.nav != NavSection::Editor || state.renaming_clip_id.is_some() {
        return false;
    }
    match &event.key {
        Key::Text(t) if t == " " => state.toggle_play(),
        Key::Left => state.seek_relative_ms(-5_000),
        Key::Right => state.seek_relative_ms(5_000),
        _ => return false,
    }
    true
}

fn main() {
    clipforge_common::init_tracing("info");
    rune::run(AppSpec::<AppState> {
        min_size: Some((960.0, 640.0)),
        scenes: &[
            ("dashboard", |s| s.nav = NavSection::Dashboard),
            ("library", |s| s.nav = NavSection::Library),
            ("editor", |s| s.nav = NavSection::Editor),
            ("recording", |s| s.nav = NavSection::Recording),
            ("uploads", |s| s.nav = NavSection::Uploads),
            ("settings", |s| s.nav = NavSection::Settings),
        ],
        setup: Some(setup_worker),
        poll: Some(poll_worker),
        on_key: Some(on_key),
        ..AppSpec::new(
            "ClipForge",
            (1240.0, 800.0),
            AppState::default,
            views::root_view,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_common::CaptureStatus;
    use rune::{Env, Runtime};

    fn click_key(rt: &mut Runtime<AppState>, key: &str) {
        let (x, y) = rt
            .find(key)
            .unwrap_or_else(|| panic!("element '{key}' not found in view"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.5);
    }

    #[test]
    fn navigates_all_six_sections() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);
        assert_eq!(rt.state.nav, NavSection::Library);

        for section in NavSection::ALL {
            click_key(&mut rt, section.key());
            assert_eq!(rt.state.nav, section);
        }
    }

    #[test]
    fn replay_buffer_toggle_and_save_adds_clip() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        let initial_clips = rt.state.clips.len();
        click_key(&mut rt, "topbar-replay-toggle");
        assert!(matches!(
            rt.state.capture_status,
            CaptureStatus::ReplayBuffering { .. }
        ));

        click_key(&mut rt, "topbar-save-clip");
        assert_eq!(rt.state.clips.len(), initial_clips + 1);

        click_key(&mut rt, "topbar-replay-toggle");
        assert_eq!(rt.state.capture_status, CaptureStatus::Idle);
    }

    #[test]
    fn editor_trim_controls_and_export_validation() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Editor.key());
        let initial_in = rt.state.editor.trim_start_ms;
        let initial_out = rt.state.editor.trim_end_ms;

        click_key(&mut rt, "in-plus");
        assert_eq!(rt.state.editor.trim_start_ms, initial_in + 1_000);

        click_key(&mut rt, "out-minus");
        assert_eq!(rt.state.editor.trim_end_ms, initial_out - 1_000);

        click_key(&mut rt, "trim-export");
        assert!(rt.state.editor.last_export_path.is_some());
    }

    #[test]
    fn library_filter_and_view_mode_toggle() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Library.key());
        click_key(&mut rt, "mode-list");
        assert_eq!(rt.state.library_mode, state::LibraryViewMode::List);

        click_key(&mut rt, "filter-fav");
        assert_eq!(rt.state.library_filter, state::LibraryFilter::Favorites);
        assert_eq!(rt.state.filtered_clips().len(), 1);
    }

    #[test]
    fn settings_toggle_updates_config() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Settings.key());
        let prev = rt.state.config.auto_generate_thumbnails;
        click_key(&mut rt, "settings-thumbs-switch");
        assert_eq!(rt.state.config.auto_generate_thumbnails, !prev);
    }

    #[test]
    fn sample_clips_distinguished_and_recovery_clears_error() {
        let env = Env::new(true, 1.0);
        let initial = AppState {
            capture_status: CaptureStatus::Error {
                message: "GSR crashed".to_string(),
            },
            ..AppState::default()
        };
        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        assert!(rt.state.using_sample_clips);
        assert!(rt.state.clips.iter().all(state::is_sample_clip));

        click_key(&mut rt, NavSection::Recording.key());
        click_key(&mut rt, "rec-clear-err");
        assert_eq!(rt.state.capture_status, CaptureStatus::Idle);
    }

    #[test]
    fn library_click_selects_and_double_click_opens_editor() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        let second_id = rt.state.clips[1].id;
        click_key(&mut rt, NavSection::Library.key());
        click_key(&mut rt, &format!("lib-card-{second_id}"));
        assert_eq!(rt.state.editor.selected_clip_id, Some(second_id));
        assert!(rt.find("lib-selected-detail").is_some());
        assert_eq!(rt.state.nav, NavSection::Library);

        // Double-click the card to open it in the Video Editor.
        click_key(&mut rt, &format!("lib-card-{second_id}"));
        assert_eq!(rt.state.nav, NavSection::Editor);
        assert_eq!(rt.state.editor.selected_clip_id, Some(second_id));
    }

    #[test]
    fn library_context_actions_and_editor_playback_helpers() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        let second_id = rt.state.clips[1].id;
        click_key(&mut rt, NavSection::Library.key());
        click_key(&mut rt, &format!("lib-card-{second_id}"));

        let was_fav = rt.state.favorite_ids.contains(&second_id);
        click_key(&mut rt, "detail-fav");
        assert_eq!(rt.state.favorite_ids.contains(&second_id), !was_fav);

        click_key(&mut rt, "detail-player");
        assert!(rt.state.status_banner.is_some());

        click_key(&mut rt, "detail-edit");
        assert_eq!(rt.state.nav, NavSection::Editor);
        assert_eq!(rt.state.editor.selected_clip_id, Some(second_id));

        rt.state.toggle_play();
        assert!(rt.state.editor.playing);
        rt.state.seek_editor(5_000);
        assert_eq!(rt.state.editor.playhead_ms, 5_000);
        rt.state.set_trim_start(2_000);
        assert_eq!(rt.state.editor.trim_start_ms, 2_000);
        rt.state.set_trim_end(10_000);
        assert_eq!(rt.state.editor.trim_end_ms, 10_000);
        rt.state.restart_editor();
        assert_eq!(rt.state.editor.playhead_ms, 2_000);
        rt.state.reset_trim();
        assert_eq!(rt.state.editor.trim_start_ms, 0);
    }

    fn make_real_clip(
        title: &str,
        duration_ms: u64,
        size_bytes: u64,
        mins_ago: i64,
    ) -> clipforge_common::ClipMetadata {
        let id = clipforge_common::ClipId::new();
        clipforge_common::ClipMetadata {
            id,
            title: title.to_string(),
            game: Some(clipforge_common::GameInfo {
                id: "cs2".to_string(),
                name: "Counter-Strike 2".to_string(),
                executable: None,
                process_name: None,
            }),
            file_path: std::path::PathBuf::from(format!("/tmp/clipforge-test/{id}.mp4")),
            thumbnail_path: None,
            created_at: chrono::Utc::now() - chrono::Duration::minutes(mins_ago),
            duration_ms,
            width: 1920,
            height: 1080,
            fps: 60.0,
            size_bytes,
            codec: "h264".to_string(),
            audio_tracks: vec![],
            tags: vec![],
            cloud_share: None,
            notes: None,
        }
    }

    #[test]
    fn trash_and_restore_with_confirmation_updates_library_and_closes_player() {
        let env = Env::new(true, 1.0);
        let c1 = make_real_clip("Alpha Match", 15_000, 10_000_000, 10);
        let c2 = make_real_clip("Beta Match", 30_000, 20_000_000, 5);
        let id1 = c1.id;
        let id2 = c2.id;

        let initial = AppState {
            using_sample_clips: false,
            clips: vec![c1, c2],
            editor: state::EditorState {
                selected_clip_id: Some(id1),
                playing: true,
                has_video_frame: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Library.key());
        // Request Move to Trash -> confirmation bar appears, clip is NOT trashed yet.
        click_key(&mut rt, &format!("trash-{id1}"));
        assert!(rt.find("lib-confirm-bar").is_some());
        assert_eq!(rt.state.clips.len(), 2);

        // Cancel confirmation.
        click_key(&mut rt, "confirm-cancel-btn");
        assert!(rt.find("lib-confirm-bar").is_none());
        assert_eq!(rt.state.clips.len(), 2);

        // Confirm Move to Trash.
        click_key(&mut rt, &format!("trash-{id1}"));
        click_key(&mut rt, "confirm-action-btn");
        assert_eq!(rt.state.clips.len(), 1);
        assert_eq!(rt.state.clips[0].id, id2);
        assert_eq!(rt.state.trashed_clips.len(), 1);
        assert_eq!(rt.state.trashed_clips[0].id, id1);
        assert!(!rt.state.editor.playing);
        assert!(!rt.state.editor.has_video_frame);
        assert_eq!(rt.state.editor.selected_clip_id, None);

        // Switch to Trash view and restore with confirmation.
        click_key(&mut rt, "filter-trash");
        assert_eq!(rt.state.filtered_clips().len(), 1);
        click_key(&mut rt, &format!("restore-{id1}"));
        assert!(rt.find("lib-confirm-bar").is_some());
        click_key(&mut rt, "confirm-action-btn");

        assert_eq!(rt.state.trashed_clips.len(), 0);
        assert_eq!(rt.state.clips.len(), 2);
    }

    #[test]
    fn permanent_delete_requires_confirmation_and_removes_clip() {
        let env = Env::new(true, 1.0);
        let c1 = make_real_clip("Delete Me", 12_000, 8_000_000, 1);
        let id1 = c1.id;

        let initial = AppState {
            using_sample_clips: false,
            clips: vec![c1],
            favorite_ids: [id1].into_iter().collect(),
            editor: state::EditorState {
                selected_clip_id: Some(id1),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Library.key());
        click_key(&mut rt, "detail-perm-delete");
        assert!(rt.find("lib-confirm-bar").is_some());
        assert_eq!(rt.state.clips.len(), 1);

        click_key(&mut rt, "confirm-action-btn");
        assert!(rt.state.clips.is_empty());
        assert!(rt.state.trashed_clips.is_empty());
        assert!(!rt.state.favorite_ids.contains(&id1));
    }

    #[test]
    fn multi_select_ctrl_shift_and_bulk_actions() {
        let env = Env::new(true, 1.0);
        let c1 = make_real_clip("Clip One", 10_000, 5_000_000, 30);
        let c2 = make_real_clip("Clip Two", 20_000, 10_000_000, 20);
        let c3 = make_real_clip("Clip Three", 30_000, 15_000_000, 10);
        let (id1, id2, id3) = (c1.id, c2.id, c3.id);

        let initial = AppState {
            using_sample_clips: false,
            clips: vec![c1, c2, c3],
            favorite_ids: Default::default(),
            selected_clip_ids: Default::default(),
            editor: state::EditorState {
                selected_clip_id: None,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);
        click_key(&mut rt, NavSection::Library.key());

        // Plain click selects c3 (newest), then Ctrl-click toggles c1, then Shift-click selects range.
        rt.update(|s, _| s.select_clip_with_mods(id3, rune::Mods::default()));
        assert_eq!(rt.state.selected_clip_ids.len(), 1);
        assert!(rt.state.selected_clip_ids.contains(&id3));

        rt.update(|s, _| {
            s.select_clip_with_mods(
                id1,
                rune::Mods {
                    ctrl: true,
                    cmd: true,
                    ..Default::default()
                },
            );
        });
        assert_eq!(rt.state.selected_clip_ids.len(), 2);

        // Shift-click from anchor id1 to id3 selects all 3 in order.
        rt.update(|s, _| {
            s.select_clip_with_mods(
                id3,
                rune::Mods {
                    shift: true,
                    ..Default::default()
                },
            );
        });
        assert_eq!(rt.state.selected_clip_ids.len(), 3);
        rt.frame(0.5);

        // Bulk favorite all 3.
        click_key(&mut rt, "bulk-fav");
        assert!(rt.state.favorite_ids.contains(&id1));
        assert!(rt.state.favorite_ids.contains(&id2));
        assert!(rt.state.favorite_ids.contains(&id3));

        // Checkbox toggle removes id2.
        click_key(&mut rt, &format!("select-check-{id2}"));
        assert_eq!(rt.state.selected_clip_ids.len(), 2);
        assert!(!rt.state.selected_clip_ids.contains(&id2));

        // Bulk Move to Trash with confirmation moves id1 & id3, leaving id2.
        click_key(&mut rt, "bulk-trash");
        assert!(rt.find("lib-confirm-bar").is_some());
        click_key(&mut rt, "confirm-action-btn");
        assert_eq!(rt.state.clips.len(), 1);
        assert_eq!(rt.state.clips[0].id, id2);
        assert_eq!(rt.state.trashed_clips.len(), 2);

        // Bulk Permanent Delete from Trash.
        click_key(&mut rt, "filter-trash");
        rt.update(|s, _| s.select_all_filtered());
        rt.frame(0.5);
        click_key(&mut rt, "bulk-perm-delete");
        click_key(&mut rt, "confirm-action-btn");
        assert!(rt.state.trashed_clips.is_empty());
    }

    #[test]
    fn library_refreshes_immediately_and_clears_last_clip_on_worker_event() {
        let mut st = AppState::default();
        assert!(st.using_sample_clips);
        let c1 = make_real_clip("Real Clip", 10_000, 12_000_000, 2);
        let id1 = c1.id;

        // First real load replaces sample clips.
        let mut origins = std::collections::HashMap::new();
        origins.insert(id1, clipforge_library::ClipOrigin::Recorded);
        st.clips = vec![c1];
        st.using_sample_clips = false;
        st.clip_origins = origins;
        assert_eq!(st.clips.len(), 1);

        // Trashing the last active clip updates clips & trashed_clips immediately and closes player.
        st.editor.selected_clip_id = Some(id1);
        st.editor.playing = true;
        st.request_move_to_trash(vec![id1]);
        st.confirm_pending_action();
        assert!(st.clips.is_empty());
        assert_eq!(st.trashed_clips.len(), 1);
        assert!(!st.editor.playing);

        // Permanently deleting from trash leaves both empty.
        st.request_permanent_delete(vec![id1]);
        st.confirm_pending_action();
        assert!(st.clips.is_empty());
        assert!(st.trashed_clips.is_empty());
    }

    #[test]
    fn rename_clip_sort_filter_and_context_menu() {
        let env = Env::new(true, 1.0);
        let mut c1 = make_real_clip("Zebra Clip", 10_000, 50_000_000, 30);
        c1.tags.push("trimmed".to_string());
        let c2 = make_real_clip("Alpha Clip", 40_000, 10_000_000, 5);
        let (id1, id2) = (c1.id, c2.id);

        let initial = AppState {
            using_sample_clips: false,
            clips: vec![c1, c2],
            editor: state::EditorState {
                selected_clip_id: Some(id1),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);
        click_key(&mut rt, NavSection::Library.key());

        // Sorting checks.
        click_key(&mut rt, "sort-title");
        assert_eq!(rt.state.filtered_clips()[0].id, id2); // Alpha < Zebra
        click_key(&mut rt, "sort-largest");
        assert_eq!(rt.state.filtered_clips()[0].id, id1); // 50MB > 10MB
        click_key(&mut rt, "sort-longest");
        assert_eq!(rt.state.filtered_clips()[0].id, id2); // 40s > 10s
        click_key(&mut rt, "sort-oldest");
        assert_eq!(rt.state.filtered_clips()[0].id, id1); // 30m ago < 5m ago
        click_key(&mut rt, "sort-newest");
        assert_eq!(rt.state.filtered_clips()[0].id, id2);

        // Origin filters & search.
        click_key(&mut rt, "filter-trimmed");
        assert_eq!(rt.state.filtered_clips().len(), 1);
        assert_eq!(rt.state.filtered_clips()[0].id, id1);
        click_key(&mut rt, "filter-recorded");
        assert_eq!(rt.state.filtered_clips().len(), 1);
        assert_eq!(rt.state.filtered_clips()[0].id, id2);
        click_key(&mut rt, "filter-all");

        rt.update(|s, _| s.library_search = rune::TextEdit::new("zebra"));
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.filtered_clips().len(), 1);
        click_key(&mut rt, "lib-search-clear");
        assert_eq!(rt.state.filtered_clips().len(), 2);

        // Context menu -> Rename -> empty rejected -> valid title committed.
        click_key(&mut rt, &format!("menu-btn-{id1}"));
        assert!(rt.find("context-menu").is_some());
        click_key(&mut rt, "ctx-rename");
        assert!(rt.find("rename-input").is_some());

        rt.update(|s, _| s.rename_edit = rune::TextEdit::new("   "));
        rt.frame(1.0 / 60.0);
        click_key(&mut rt, "rename-save");
        assert_eq!(
            rt.state.find_clip(id1).map(|c| c.title.as_str()),
            Some("Zebra Clip")
        );

        rt.update(|s, _| s.rename_edit = rune::TextEdit::new("Renamed Clutch"));
        rt.frame(1.0 / 60.0);
        click_key(&mut rt, "rename-save");
        assert_eq!(
            rt.state.find_clip(id1).map(|c| c.title.as_str()),
            Some("Renamed Clutch")
        );
    }

    #[test]
    fn play_in_editor_audio_zoom_cancel_and_keyboard_nav() {
        let env = Env::new(true, 1.0);
        let c1 = make_real_clip("Editor Clip", 30_000, 12_000_000, 2);
        let id1 = c1.id;

        let initial = AppState {
            using_sample_clips: false,
            clips: vec![c1],
            ..Default::default()
        };
        let mut rt = Runtime::new(initial, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        // Library "Play" button opens editor and starts playback immediately.
        click_key(&mut rt, NavSection::Library.key());
        click_key(&mut rt, &format!("play-{id1}"));
        assert_eq!(rt.state.nav, NavSection::Editor);
        assert_eq!(rt.state.editor.selected_clip_id, Some(id1));
        assert!(rt.state.editor.playing);

        // Volume, mute, and timeline zoom clamping.
        rt.state.set_volume(1.5);
        assert_eq!(rt.state.editor.volume, 1.0);
        rt.state.set_volume(0.35);
        assert!((rt.state.editor.volume - 0.35).abs() < f32::EPSILON);
        rt.state.toggle_mute();
        assert!(rt.state.editor.muted);
        rt.state.set_timeline_zoom(12.0);
        assert_eq!(rt.state.editor.timeline_zoom, 8.0);
        rt.state.set_timeline_zoom(0.2);
        assert_eq!(rt.state.editor.timeline_zoom, 1.0);

        // Keyboard navigation in Editor: Space toggles play/pause, Right/Left seek +/-5s.
        let mut tx = Tx::default();
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Text(" ".into()),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert!(!rt.state.editor.playing);

        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Right,
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert_eq!(rt.state.editor.playhead_ms, 5_000);

        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Left,
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert_eq!(rt.state.editor.playhead_ms, 0);

        // Export cancel clears exporting and export_progress.
        rt.state.editor.exporting = true;
        rt.state.editor.export_progress = Some(0.42);
        rt.state.cancel_export();
        assert!(!rt.state.editor.exporting);
        assert_eq!(rt.state.editor.export_progress, None);
    }

    #[test]
    fn focused_hotkeys_f9_f10_f11_dispatch_capture_actions_in_any_view() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);
        assert_eq!(rt.state.nav, NavSection::Library);

        let mut tx = Tx::default();
        let initial_clips = rt.state.clips.len();

        // F10 toggles replay buffer on and off from Library view.
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(10),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert!(matches!(
            rt.state.capture_status,
            CaptureStatus::ReplayBuffering { .. }
        ));

        // F9 saves a replay clip.
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(9),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert_eq!(rt.state.clips.len(), initial_clips + 1);

        // F10 toggles replay buffer back to Idle.
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(10),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert_eq!(rt.state.capture_status, CaptureStatus::Idle);

        // F11 toggles recording on and off.
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(11),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert!(matches!(
            rt.state.capture_status,
            CaptureStatus::Recording { .. }
        ));
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(11),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert_eq!(rt.state.capture_status, CaptureStatus::Idle);

        // Custom modifier hotkey requires the exact modifier mask.
        rt.state.hotkeys.save_replay = "Ctrl+Alt+F9".to_string();
        assert!(!on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(9),
                mods: rune::Mods::default(),
            },
            &mut tx
        ));
        assert!(on_key(
            &mut rt.state,
            &KeyEvent {
                key: Key::Function(9),
                mods: rune::Mods {
                    ctrl: true,
                    alt: true,
                    ..Default::default()
                },
            },
            &mut tx
        ));
        assert_eq!(rt.state.clips.len(), initial_clips + 2);
    }

    #[test]
    fn hotkey_status_labels_reflect_registration_state() {
        use clipforge_capture::GlobalHotkeyStatus;

        let mut st = AppState::default();
        assert!(st.hotkey_status.label().starts_with("Focused-Window Only"));

        st.hotkey_status = GlobalHotkeyStatus::PermissionPending;
        assert_eq!(st.hotkey_status.label(), "Permission Pending");

        st.hotkey_status = GlobalHotkeyStatus::WaylandPortalActive;
        assert_eq!(
            st.hotkey_status.label(),
            "GlobalShortcuts (Wayland Portal Active)"
        );

        st.hotkey_status = GlobalHotkeyStatus::RegistrationError {
            message: "portal denied".to_string(),
        };
        assert_eq!(
            st.hotkey_status.label(),
            "Registration Error: portal denied"
        );
    }

    fn type_keys(rt: &mut Runtime<AppState>, text: &str) {
        for ch in text.chars() {
            rt.key(KeyEvent {
                key: Key::Text(ch.to_string()),
                mods: rune::Mods::default(),
            });
            rt.frame(1.0 / 60.0);
        }
    }

    #[test]
    fn uploads_text_inputs_click_type_paste_select_tab_and_mask_password() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        click_key(&mut rt, NavSection::Uploads.key());
        assert_eq!(rt.state.nav, NavSection::Uploads);

        // 1. Click Server Endpoint URL field and type.
        click_key(&mut rt, "server-url-input");
        type_keys(&mut rt, "https://clips.example.com");
        assert_eq!(rt.state.server_url_edit.text, "https://clips.example.com");

        // 2. Select all (Ctrl/Cmd+A) and replace via paste.
        rt.key(KeyEvent {
            key: Key::Command('a'),
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        rt.paste("https://new.example.com");
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.server_url_edit.text, "https://new.example.com");

        // 3. Tab to API Bearer Token field and paste token; verify masking.
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        rt.paste("cf_tok_secret");
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.api_token_edit.text, "cf_tok_secret");

        // 4. Tab to Username, type, select last 2 chars with Shift+Left, and replace.
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        type_keys(&mut rt, "alixx");
        assert_eq!(rt.state.server_user_edit.text, "alixx");
        for _ in 0..2 {
            rt.key(KeyEvent {
                key: Key::Left,
                mods: rune::Mods {
                    shift: true,
                    ..Default::default()
                },
            });
            rt.frame(1.0 / 60.0);
        }
        type_keys(&mut rt, "ce");
        assert_eq!(rt.state.server_user_edit.text, "alice");

        // 5. Tab to Password, type via Key::Text, and verify password masking.
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        type_keys(&mut rt, "hunter2");
        assert_eq!(rt.state.server_pass_edit.text, "hunter2");

        // Shift+Tab navigates back to Username and allows editing.
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: rune::Mods {
                shift: true,
                ..Default::default()
            },
        });
        rt.frame(1.0 / 60.0);
        type_keys(&mut rt, "!");
        assert_eq!(rt.state.server_user_edit.text, "alice!");

        // Secure fields must not expose plaintext in accessibility/element inspection.
        let elements = rt.elements();
        assert!(elements.iter().all(|e| {
            e.text.as_deref() != Some("hunter2") && e.text.as_deref() != Some("cf_tok_secret")
        }));
    }

    #[test]
    fn settings_and_library_text_inputs_accept_typing_and_tab() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(AppState::default(), views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        // Library search field accepts click + typing.
        click_key(&mut rt, "lib-search-input");
        type_keys(&mut rt, "clutch");
        assert_eq!(rt.state.library_search.text, "clutch");

        // Settings search field accepts click + typing.
        click_key(&mut rt, NavSection::Settings.key());
        click_key(&mut rt, "settings-search");
        type_keys(&mut rt, "codec");
        assert_eq!(rt.state.settings_search.text, "codec");
        assert!(rt.find("settings-search-group").is_some());

        // Clear search and test Storage clips dir input.
        click_key(&mut rt, "settings-clear-search");
        click_key(&mut rt, crate::state::SettingsTab::Storage.key());
        click_key(&mut rt, "settings-clips-dir");
        rt.key(KeyEvent {
            key: Key::Command('a'),
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        rt.paste("/tmp/my-clips");
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.clips_dir_edit.text, "/tmp/my-clips");

        // Settings Sharing tab: server URL + masked API token via Tab.
        click_key(&mut rt, crate::state::SettingsTab::Sharing.key());
        click_key(&mut rt, "settings-server-url");
        rt.key(KeyEvent {
            key: Key::Command('a'),
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        type_keys(&mut rt, "https://clips.emkacz.dev");
        assert_eq!(rt.state.server_url_edit.text, "https://clips.emkacz.dev");

        rt.key(KeyEvent {
            key: Key::Tab,
            mods: rune::Mods::default(),
        });
        rt.frame(1.0 / 60.0);
        rt.paste("tok_12345");
        rt.frame(1.0 / 60.0);
        assert_eq!(rt.state.api_token_edit.text, "tok_12345");
        assert!(
            rt.elements()
                .iter()
                .all(|e| e.text.as_deref() != Some("tok_12345"))
        );
    }

    #[test]
    fn uploads_visibility_update_requires_server_confirmation_and_persists_on_refresh() {
        use chrono::Utc;
        use clipforge_cloud_client::{RemoteClipSummary, ShareVisibility};
        use clipforge_common::{ClipId, CloudShareInfo};

        let clip_id = ClipId::new();
        let now = Utc::now();
        let initial_summary = RemoteClipSummary {
            share_id: "shr_tok".to_string(),
            url: "https://clips.example.com/c/shr_tok".to_string(),
            uploaded_at: now,
            expires_at: None,
            clip_id,
            title: "Clutch".to_string(),
            visibility: ShareVisibility::Public,
            size_bytes: 1024,
        };

        let mut st = AppState {
            server_connected: true,
            ..Default::default()
        };
        st.uploads.clear();
        st.remote_shares.clear();
        st.remote_shares.push(initial_summary.clone());
        st.uploads.push(crate::state::UploadEntry {
            clip_id,
            title: "Clutch".to_string(),
            game_name: "Valorant".to_string(),
            status: crate::state::UploadStatus::Completed,
            share: Some(CloudShareInfo::from(initial_summary.clone())),
            is_public: true,
        });

        // Failed server update preserves existing visibility and surfaces error.
        st.apply_worker_event(crate::worker::WorkerEvent::VisibilityUpdated {
            clip_id,
            result: Err("forbidden".to_string()),
        });
        assert!(st.uploads[0].is_public);
        assert_eq!(st.remote_shares[0].visibility, ShareVisibility::Public);
        assert_eq!(st.server_error.as_deref(), Some("forbidden"));
        assert!(
            st.status_banner
                .as_deref()
                .is_some_and(|b| b.contains("Visibility update failed"))
        );

        // Confirmed server update updates both remote_shares and uploads.
        let mut updated = initial_summary;
        updated.visibility = ShareVisibility::Private;
        st.apply_worker_event(crate::worker::WorkerEvent::VisibilityUpdated {
            clip_id,
            result: Ok(updated.clone()),
        });
        assert!(!st.uploads[0].is_public);
        assert_eq!(st.remote_shares[0].visibility, ShareVisibility::Private);
        assert!(st.server_error.is_none());

        // Refreshing from server keeps the confirmed visibility.
        st.apply_worker_event(crate::worker::WorkerEvent::RemoteSharesLoaded(Ok(vec![
            updated,
        ])));
        assert!(!st.uploads[0].is_public);
        assert_eq!(st.remote_shares[0].visibility, ShareVisibility::Private);
    }

    #[test]
    fn uploads_remote_delete_preserves_clip_on_error_and_removes_on_confirmed_ok() {
        use chrono::Utc;
        use clipforge_cloud_client::{RemoteClipSummary, ShareVisibility};
        use clipforge_common::{ClipId, CloudShareInfo};

        let clip_id = ClipId::new();
        let now = Utc::now();
        let summary = RemoteClipSummary {
            share_id: "shr_tok".to_string(),
            url: "https://clips.example.com/c/shr_tok".to_string(),
            uploaded_at: now,
            expires_at: None,
            clip_id,
            title: "Ace".to_string(),
            visibility: ShareVisibility::Public,
            size_bytes: 2048,
        };

        let mut st = AppState {
            server_connected: true,
            ..Default::default()
        };
        st.uploads.clear();
        st.remote_shares.clear();
        st.selected_upload_id = Some(clip_id);
        st.remote_shares.push(summary.clone());
        st.uploads.push(crate::state::UploadEntry {
            clip_id,
            title: "Ace".to_string(),
            game_name: "CS2".to_string(),
            status: crate::state::UploadStatus::Completed,
            share: Some(CloudShareInfo::from(summary)),
            is_public: true,
        });

        st.request_delete_remote_share(clip_id);
        assert_eq!(st.confirm_remote_delete, Some(clip_id));

        // Server error preserves the clip in both remote_shares and uploads.
        st.apply_worker_event(crate::worker::WorkerEvent::RemoteShareDeleted {
            clip_id,
            result: Err("s3 delete failed".to_string()),
        });
        assert_eq!(st.remote_shares.len(), 1);
        assert_eq!(st.uploads.len(), 1);
        assert_eq!(st.confirm_remote_delete, None);
        assert_eq!(st.server_error.as_deref(), Some("s3 delete failed"));
        assert!(
            st.status_banner
                .as_deref()
                .is_some_and(|b| b.contains("Remote delete failed"))
        );

        // Confirmed server Ok removes the clip and clears selection.
        st.apply_worker_event(crate::worker::WorkerEvent::RemoteShareDeleted {
            clip_id,
            result: Ok(()),
        });
        assert!(st.remote_shares.is_empty());
        assert!(st.uploads.is_empty());
        assert_eq!(st.selected_upload_id, None);
        assert!(st.server_error.is_none());
    }

    #[test]
    fn uploads_refreshed_unshared_remote_clip_retains_delete_and_visibility_actions() {
        use chrono::Utc;
        use clipforge_cloud_client::{RemoteClipSummary, ShareVisibility};
        use clipforge_common::ClipId;

        let clip_id = ClipId::new();
        let mut st = AppState {
            nav: NavSection::Uploads,
            server_connected: true,
            selected_upload_id: Some(clip_id),
            ..Default::default()
        };
        st.config.server_url = Some("https://clips.example.com".to_string());

        // Simulate a refreshed remote clip whose share link was revoked (share_id is empty).
        st.apply_worker_event(crate::worker::WorkerEvent::RemoteSharesLoaded(Ok(vec![
            RemoteClipSummary {
                share_id: String::new(),
                url: String::new(),
                uploaded_at: Utc::now(),
                expires_at: None,
                clip_id,
                title: "Restored Remote Clip".to_string(),
                visibility: ShareVisibility::Private,
                size_bytes: 4096,
            },
        ])));

        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(st, views::root_view, (1240.0, 800.0), env);
        rt.frame(0.0);

        assert!(
            rt.find("insp-delete-btn").is_some(),
            "Delete from Server button must remain available for unshared/refreshed remote clips"
        );
        assert!(rt.find("insp-vis-public").is_some());
        assert!(rt.find("insp-vis-private").is_some());
        assert!(rt.find("insp-create-link").is_some());

        click_key(&mut rt, "insp-delete-btn");
        assert_eq!(rt.state.confirm_remote_delete, Some(clip_id));
        assert!(rt.find("insp-delete-confirm").is_some());
    }
}
