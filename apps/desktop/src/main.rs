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
        rt.frame(1.0 / 60.0);
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
}
