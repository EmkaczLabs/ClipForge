//! ClipForge Desktop — native GPU-rendered Linux game clipping app built with Rune.

#![forbid(unsafe_code)]

mod components;
mod config;
mod state;
mod theme;
mod views;
mod worker;

use std::time::Instant;

use rune::{AppSpec, Tx, Waker};
use state::{AppState, NavSection};
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
        assert_eq!(rt.state.nav, NavSection::Dashboard);

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

        click_key(&mut rt, "dash-clear-err");
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
}
