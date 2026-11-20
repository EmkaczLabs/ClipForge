//! Settings view: Obsidian Forge split-pane configuration, search, and dependency diagnostics.

use clipforge_common::{
    AudioCodec, CaptureTarget, ContainerFormat, EncoderMode, QualityPreset, VideoCodec,
};
use rune::{Color, Cursor, El, col, el, row, text};

use crate::components::{
    badge, btn_primary, btn_secondary, card, empty_state, kv_row, panel, pill_option, search_field,
    section_header, segmented_control, segmented_item, settings_row, status_pill, text_input_box,
    toggle_switch,
};
use crate::config::config_file_path;
use crate::state::{AppState, NavSection, SettingsTab};
use crate::theme::{Palette, radius, space, typography};
use crate::worker::WorkerCommand;

struct SettingRowSpec {
    title: &'static str,
    description: &'static str,
    category: &'static str,
    element: El,
}

impl SettingRowSpec {
    fn matches(&self, query: &str) -> bool {
        self.title.to_lowercase().contains(query)
            || self.description.to_lowercase().contains(query)
            || self.category.to_lowercase().contains(query)
    }
}

pub fn view(p: &Palette, s: &AppState) -> El {
    let query = s.settings_search.text.trim().to_lowercase();

    row()
        .key("settings-view")
        .w_full()
        .gap(space::LG)
        .child(category_rail(p, s))
        .child(content_pane(p, s, &query))
}

fn category_rail(p: &Palette, s: &AppState) -> El {
    let all_deps_ok = s.deps.gsr_available && s.deps.ffmpeg_available && s.deps.ffprobe_available;
    let session_label = if s.is_wayland { "Wayland" } else { "X11" };

    panel(p, "settings-nav-rail")
        .w(232.0)
        .padding(space::MD)
        .gap(space::MD)
        .child(
            col()
                .key("settings-rail-header")
                .gap(space::SM)
                .child(
                    row()
                        .key("settings-rail-title-row")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text("SETTINGS")
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        )
                        .child(badge(
                            "settings-version-badge",
                            concat!("v", env!("CARGO_PKG_VERSION")),
                            p.text_secondary,
                            p.bg_surface,
                        )),
                )
                .child(search_field(
                    p,
                    "settings-search",
                    &s.settings_search,
                    "Filter settings...",
                )),
        )
        .child(
            col()
                .key("settings-rail-tabs")
                .w_full()
                .gap(space::XS)
                .children(SettingsTab::ALL.into_iter().map(|tab| {
                    let active = s.settings_tab == tab && s.settings_search.text.trim().is_empty();
                    let trailing = match tab {
                        SettingsTab::Audio if !s.config.capture.audio_sources.is_empty() => {
                            Some(status_dot(p, "audio-dot", p.success))
                        }
                        SettingsTab::Shortcuts => Some(badge(
                            "hk-badge",
                            &format!("{}/{}", s.hotkeys.save_replay, s.hotkeys.toggle_record),
                            p.text_muted,
                            p.bg_lowest,
                        )),
                        SettingsTab::Sharing if s.server_connected => {
                            Some(status_dot(p, "share-dot", p.success))
                        }
                        SettingsTab::About => Some(status_dot(
                            p,
                            "deps-dot",
                            if all_deps_ok { p.success } else { p.warning },
                        )),
                        _ => None,
                    };
                    category_tab_button(p, tab, active, trailing)
                })),
        )
        .child(
            col()
                .key("settings-rail-footer")
                .w_full()
                .pt(space::SM)
                .gap(space::XS)
                .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
                .child(
                    text("CONFIG TARGET")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(
                    el().key("settings-config-path-box")
                        .w_full()
                        .px(space::SM)
                        .py(6.0)
                        .radius(radius::XS)
                        .fill(p.bg_lowest)
                        .border(1.0, p.border_subtle)
                        .child(
                            text(config_file_path().display().to_string())
                                .font(typography::MONO_BADGE)
                                .color(p.text_secondary),
                        ),
                )
                .child(
                    row()
                        .key("settings-session-meta")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text(format!("{session_label} Session"))
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        )
                        .child(badge(
                            "settings-sync-badge",
                            "JSON",
                            p.accent_primary,
                            p.accent_soft,
                        )),
                ),
        )
}

fn category_tab_button(p: &Palette, tab: SettingsTab, active: bool, trailing: Option<El>) -> El {
    let fill = if active {
        p.bg_elevated
    } else {
        Color::TRANSPARENT
    };
    let border = if active {
        p.border_active
    } else {
        Color::TRANSPARENT
    };
    let fg = if active {
        p.text_primary
    } else {
        p.text_secondary
    };

    let hover_fill = if active { fill } else { p.bg_hover };

    row()
        .key(tab.key())
        .role("button")
        .label(tab.label())
        .focusable(true)
        .cursor(Cursor::Pointer)
        .w_full()
        .h(34.0)
        .px(space::SM)
        .gap(space::SM)
        .center_y()
        .justify_between()
        .radius(radius::SM)
        .fill(fill)
        .border(1.0, border)
        .hover(hover_fill)
        .press(p.bg_active)
        .on_press(move |st: &mut AppState| {
            st.settings_tab = tab;
            st.settings_search = rune::TextEdit::default();
        })
        .child(
            row()
                .key("tab-left")
                .gap(space::SM)
                .center_y()
                .child(
                    el().key("tab-indicator")
                        .size(4.0, 14.0)
                        .radius(radius::PILL)
                        .fill(if active {
                            p.accent_primary
                        } else {
                            Color::TRANSPARENT
                        }),
                )
                .child(text(tab.label()).font(typography::LABEL).color(fg)),
        )
        .children(trailing)
}

fn status_dot(p: &Palette, key: &str, color: Color) -> El {
    el().key(key)
        .size(7.0, 7.0)
        .radius(radius::PILL)
        .fill(color)
        .border(1.0, p.bg_lowest)
}

fn content_pane(p: &Palette, s: &AppState, query: &str) -> El {
    let is_searching = !query.is_empty();
    let (title, subtitle, tag) = if is_searching {
        (
            "Search Results",
            "Matching settings across all configuration categories.",
            "FILTERED",
        )
    } else {
        tab_header_meta(s.settings_tab)
    };

    let header = row()
        .key("settings-hdr-row")
        .w_full()
        .justify_between()
        .center_y()
        .gap(space::MD)
        .child(
            col()
                .key("settings-hdr-meta")
                .grow(1.0)
                .min_w(0.0)
                .gap(space::XS)
                .child(
                    row()
                        .key("settings-hdr-title-row")
                        .gap(space::SM)
                        .center_y()
                        .child(
                            text(title)
                                .font(typography::HEADLINE_MD)
                                .color(p.text_primary),
                        )
                        .child(badge(
                            "settings-hdr-tag",
                            tag,
                            p.accent_primary,
                            p.accent_soft,
                        )),
                )
                .child(
                    text(subtitle)
                        .font(typography::BODY)
                        .color(p.text_secondary),
                ),
        )
        .child(
            row()
                .key("settings-hdr-actions")
                .gap(space::SM)
                .center_y()
                .children(is_searching.then(|| {
                    btn_secondary(p, "settings-clear-search", "Clear Search", |st| {
                        st.settings_search = rune::TextEdit::default();
                    })
                }))
                .child(btn_secondary(p, "settings-reset", "Reset", |st| {
                    st.reset_settings();
                }))
                .child(btn_primary(p, "settings-save", "Save Settings", |st| {
                    st.save_settings();
                })),
        );

    let body = if is_searching {
        search_results_body(p, s, query)
    } else {
        match s.settings_tab {
            SettingsTab::General => general_tab(p, s),
            SettingsTab::Storage => storage_tab(p, s),
            SettingsTab::Recording => recording_tab(p, s),
            SettingsTab::Audio => audio_tab(p, s),
            SettingsTab::Shortcuts => shortcuts_tab(p, s),
            SettingsTab::Appearance => appearance_tab(p, s),
            SettingsTab::Sharing => sharing_tab(p, s),
            SettingsTab::About => about_tab(p, s),
        }
    };

    col()
        .key("settings-content-pane")
        .grow(1.0)
        .min_w(0.0)
        .gap(space::LG)
        .child(header)
        .child(body)
}

fn tab_header_meta(tab: SettingsTab) -> (&'static str, &'static str, &'static str) {
    match tab {
        SettingsTab::General => (
            "General",
            "Application startup, confirmation prompts, and background processing.",
            "CORE",
        ),
        SettingsTab::Storage => (
            "Storage & Library",
            "Clip output directories, SQLite library database, and config paths.",
            "STORAGE",
        ),
        SettingsTab::Recording => (
            "Recording Defaults",
            "Default capture target, resolution, frame rate, encoder, and container format.",
            "CAPTURE",
        ),
        SettingsTab::Audio => (
            "Audio",
            "Audio codec and PipeWire / PulseAudio capture sources.",
            "AUDIO",
        ),
        SettingsTab::Shortcuts => (
            "Keyboard Shortcuts",
            "Global hotkey bindings for saving replays and toggling capture.",
            "HOTKEYS",
        ),
        SettingsTab::Appearance => (
            "Appearance",
            "Interface density and Obsidian Forge visual theme.",
            "THEME",
        ),
        SettingsTab::Sharing => (
            "Sharing & Server",
            "Self-hosted ClipForge server endpoint and bearer token authentication.",
            "CLOUD",
        ),
        SettingsTab::About => (
            "About",
            "Application version, session environment, and external system dependencies.",
            "SYSTEM",
        ),
    }
}

fn search_results_body(p: &Palette, s: &AppState, query: &str) -> El {
    let mut rows = Vec::new();
    rows.extend(general_rows(p, s));
    rows.extend(storage_rows(p, s));
    rows.extend(recording_rows(p, s));
    rows.extend(audio_rows(p, s));
    rows.extend(shortcut_rows(p, s));
    rows.extend(appearance_rows(p, s));
    rows.extend(sharing_rows(p, s));

    let matched: Vec<El> = rows
        .into_iter()
        .filter(|r| r.matches(query))
        .map(|r| r.element)
        .collect();

    if matched.is_empty() {
        return empty_state(
            p,
            "settings-search-empty",
            "No matching settings",
            "No configuration options matched your search query.",
            Some(btn_secondary(
                p,
                "settings-search-empty-clear",
                "Clear Search",
                |st| st.settings_search = rune::TextEdit::default(),
            )),
        );
    }

    settings_group(p, "settings-search-group", "MATCHING OPTIONS", matched)
}

fn settings_group(p: &Palette, key: &str, group_title: &str, rows: Vec<El>) -> El {
    let count = rows.len();
    let mut items = Vec::with_capacity(count * 2);
    for (idx, row_el) in rows.into_iter().enumerate() {
        items.push(row_el);
        if idx + 1 < count {
            items.push(
                el().key(format!("{key}-div-{idx}"))
                    .w_full()
                    .h(1.0)
                    .fill(p.border_subtle),
            );
        }
    }

    col()
        .key(key)
        .w_full()
        .gap(space::SM)
        .child(
            row()
                .key(format!("{key}-hdr"))
                .w_full()
                .gap(space::SM)
                .center_y()
                .child(
                    text(group_title.to_string())
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(
                    el().key(format!("{key}-line"))
                        .grow(1.0)
                        .h(1.0)
                        .fill(p.border_subtle),
                ),
        )
        .child(card(p, &format!("{key}-card")).gap(0.0).children(items))
}

fn general_tab(p: &Palette, s: &AppState) -> El {
    let rows = general_rows(p, s);
    let mut app_rows = Vec::new();
    let mut rec_rows = Vec::new();
    for (idx, r) in rows.into_iter().enumerate() {
        if idx < 3 {
            app_rows.push(r.element);
        } else {
            rec_rows.push(r.element);
        }
    }

    col()
        .key("settings-general-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "general-app-group",
            "APPLICATION BEHAVIOR",
            app_rows,
        ))
        .child(settings_group(
            p,
            "general-capture-group",
            "REPLAY & PORTAL BEHAVIOR",
            rec_rows,
        ))
}

fn general_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    vec![
        SettingRowSpec {
            title: "Start minimized",
            description: "Launch ClipForge minimized in the background on startup",
            category: "General Application Behavior",
            element: settings_row(
                p,
                "row-start-minimized",
                "Start minimized",
                "Launch ClipForge minimized in the background on startup",
                toggle_switch(
                    p,
                    "settings-start-minimized",
                    "Start minimized",
                    s.config.start_minimized,
                    |st| st.config.start_minimized = !st.config.start_minimized,
                ),
            ),
        },
        SettingRowSpec {
            title: "Confirm before moving clips to trash",
            description: "Show a confirmation dialog before trashing or permanently deleting clips",
            category: "General Application Behavior",
            element: settings_row(
                p,
                "row-confirm-trash",
                "Confirm before moving clips to trash",
                "Show a confirmation dialog before trashing or permanently deleting clips",
                toggle_switch(
                    p,
                    "settings-confirm-trash",
                    "Confirm before moving clips to trash",
                    s.config.confirm_trash,
                    |st| st.config.confirm_trash = !st.config.confirm_trash,
                ),
            ),
        },
        SettingRowSpec {
            title: "Auto-generate thumbnails",
            description: "Extract clip thumbnails asynchronously using FFmpeg",
            category: "General Background Processing",
            element: settings_row(
                p,
                "row-auto-thumbs",
                "Auto-generate thumbnails",
                "Extract clip thumbnails asynchronously using FFmpeg",
                toggle_switch(
                    p,
                    "settings-thumbs-switch",
                    "Auto-generate thumbnails",
                    s.config.auto_generate_thumbnails,
                    |st| st.config.auto_generate_thumbnails = !st.config.auto_generate_thumbnails,
                ),
            ),
        },
        SettingRowSpec {
            title: "Auto-restart replay buffer on save",
            description: "Re-arm the GSR rolling replay buffer automatically after saving a replay",
            category: "General Replay Buffer",
            element: settings_row(
                p,
                "row-restart-replay",
                "Auto-restart replay buffer on save",
                "Re-arm the GSR rolling replay buffer automatically after saving a replay",
                toggle_switch(
                    p,
                    "settings-restart-replay",
                    "Auto-restart replay buffer on save",
                    s.config.capture.restart_replay_on_save,
                    |st| st.set_restart_replay_on_save(!st.config.capture.restart_replay_on_save),
                ),
            ),
        },
        SettingRowSpec {
            title: "Restore Wayland portal session",
            description: "Reuse saved xdg-desktop-portal screencast token across launches",
            category: "General Wayland Portal",
            element: settings_row(
                p,
                "row-restore-portal",
                "Restore Wayland portal session",
                "Reuse saved xdg-desktop-portal screencast token across launches",
                toggle_switch(
                    p,
                    "settings-restore-portal",
                    "Restore Wayland portal session",
                    s.config.capture.restore_portal_session,
                    |st| st.set_restore_portal_session(!st.config.capture.restore_portal_session),
                ),
            ),
        },
    ]
}

fn storage_tab(p: &Palette, s: &AppState) -> El {
    let rows: Vec<El> = storage_rows(p, s).into_iter().map(|r| r.element).collect();
    col()
        .key("settings-storage-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "storage-paths-group",
            "DIRECTORIES & DATABASE",
            rows,
        ))
}

fn storage_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    vec![
        SettingRowSpec {
            title: "Clips directory",
            description: "Primary directory where recorded and exported clips are stored",
            category: "Storage & Library",
            element: settings_row(
                p,
                "row-clips-dir",
                "Clips directory",
                "Primary directory where recorded and exported clips are stored",
                el().key("clips-dir-input-wrap")
                    .w(280.0)
                    .child(text_input_box(
                        p,
                        "settings-clips-dir",
                        &s.clips_dir_edit,
                        "~/Videos/ClipForge",
                    )),
            ),
        },
        SettingRowSpec {
            title: "Capture staging directory",
            description: "Output directory passed to gpu-screen-recorder",
            category: "Storage & Library",
            element: kv_row(
                p,
                "row-capture-output-dir",
                "Capture staging directory",
                s.config.capture.output_dir.display().to_string(),
            ),
        },
        SettingRowSpec {
            title: "SQLite library database",
            description: "Local SQLite index for clip metadata, games, and tags",
            category: "Storage & Library",
            element: kv_row(
                p,
                "row-db-path",
                "SQLite library database",
                s.config.database_path.display().to_string(),
            ),
        },
        SettingRowSpec {
            title: "Configuration file",
            description: "Persisted atomically as JSON on Save Settings",
            category: "Storage & Library",
            element: kv_row(
                p,
                "row-cfg-file-path",
                "Configuration file",
                config_file_path().display().to_string(),
            ),
        },
        SettingRowSpec {
            title: "Indexed library items",
            description: "Active clips and items currently in Trash",
            category: "Storage & Library",
            element: kv_row(
                p,
                "row-library-counts",
                "Indexed library items",
                format!(
                    "{} active • {} in trash",
                    s.clips.len(),
                    s.trashed_clips.len()
                ),
            ),
        },
    ]
}

fn recording_tab(p: &Palette, s: &AppState) -> El {
    let rows = recording_rows(p, s);
    let mut capture_rows = Vec::new();
    let mut encode_rows = Vec::new();
    for (idx, r) in rows.into_iter().enumerate() {
        if idx < 4 {
            capture_rows.push(r.element);
        } else {
            encode_rows.push(r.element);
        }
    }

    col()
        .key("settings-recording-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "recording-capture-group",
            "CAPTURE SOURCE & TIMING",
            capture_rows,
        ))
        .child(settings_group(
            p,
            "recording-encode-group",
            "ENCODER & FORMAT",
            encode_rows,
        ))
}

fn recording_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    let target = &s.config.capture.capture_target;
    let res = (s.config.capture.width, s.config.capture.height);
    let fps = s.config.capture.fps;
    let buf = s.config.capture.replay_buffer_secs;
    let enc = s.config.capture.encoder;
    let codec = s.config.capture.video_codec;
    let quality = s.config.capture.quality;
    let container = s.config.capture.container;

    let target_desc = if s.is_wayland {
        "Wayland active: Focused Window capture is X11-only; use Primary Screen or Portal"
    } else {
        "Default display, window, or portal source for new capture sessions"
    };

    vec![
        SettingRowSpec {
            title: "Capture target",
            description: "Default display, window, or portal source for new capture sessions",
            category: "Recording Defaults",
            element: settings_row(
                p,
                "row-capture-target",
                "Capture target",
                target_desc,
                row()
                    .key("target-opts")
                    .gap(space::XS)
                    .child(pill_option(
                        p,
                        "settings-target-primary",
                        "Primary Screen",
                        matches!(target, CaptureTarget::PrimaryScreen),
                        |st| st.set_capture_target(CaptureTarget::PrimaryScreen),
                    ))
                    .child(pill_option(
                        p,
                        "settings-target-portal",
                        "Portal",
                        matches!(target, CaptureTarget::Portal),
                        |st| st.set_capture_target(CaptureTarget::Portal),
                    ))
                    .child(if s.is_wayland {
                        disabled_pill(p, "settings-target-window-disabled", "Window (X11)")
                    } else {
                        pill_option(
                            p,
                            "settings-target-window",
                            "Focused Window",
                            matches!(target, CaptureTarget::FocusedWindow),
                            |st| st.set_capture_target(CaptureTarget::FocusedWindow),
                        )
                    }),
            ),
        },
        SettingRowSpec {
            title: "Replay buffer duration",
            description: "Rolling in-memory capture window saved when triggering a replay",
            category: "Recording Defaults",
            element: settings_row(
                p,
                "row-replay-secs",
                "Replay buffer duration",
                "Rolling in-memory capture window saved when triggering a replay",
                row().key("replay-secs-opts").gap(space::XS).children(
                    [30u32, 60, 120, 180, 300].into_iter().map(|secs| {
                        pill_option(
                            p,
                            &format!("settings-buf-{secs}"),
                            &format!("{secs}s"),
                            buf == secs,
                            move |st| st.set_replay_buffer_secs(secs),
                        )
                    }),
                ),
            ),
        },
        SettingRowSpec {
            title: "Frame rate",
            description: "Target capture frame rate in frames per second",
            category: "Recording Defaults",
            element: settings_row(
                p,
                "row-fps",
                "Frame rate",
                "Target capture frame rate in frames per second",
                row().key("fps-opts").gap(space::XS).children(
                    [30u32, 60, 120, 144].into_iter().map(|val| {
                        pill_option(
                            p,
                            &format!("settings-fps-{val}"),
                            &format!("{val} FPS"),
                            fps == val,
                            move |st| st.set_capture_fps(val),
                        )
                    }),
                ),
            ),
        },
        SettingRowSpec {
            title: "Output resolution",
            description: "Scale output video or keep native display resolution",
            category: "Recording Defaults",
            element: settings_row(
                p,
                "row-resolution",
                "Output resolution",
                "Scale output video or keep native display resolution",
                row()
                    .key("res-opts")
                    .gap(space::XS)
                    .child(pill_option(
                        p,
                        "settings-res-native",
                        "Native",
                        res == (None, None),
                        |st| st.set_capture_resolution(None, None),
                    ))
                    .child(pill_option(
                        p,
                        "settings-res-1080p",
                        "1080p",
                        res == (Some(1920), Some(1080)),
                        |st| st.set_capture_resolution(Some(1920), Some(1080)),
                    ))
                    .child(pill_option(
                        p,
                        "settings-res-1440p",
                        "1440p",
                        res == (Some(2560), Some(1440)),
                        |st| st.set_capture_resolution(Some(2560), Some(1440)),
                    )),
            ),
        },
        SettingRowSpec {
            title: "Default encoder",
            description: "Hardware GPU encoding vs software CPU fallback",
            category: "Recording Defaults Encoder",
            element: settings_row(
                p,
                "row-encoder",
                "Default encoder",
                "Hardware GPU encoding vs software CPU fallback",
                segmented_control(
                    p,
                    "settings-enc-seg",
                    vec![
                        segmented_item(
                            p,
                            "settings-enc-gpu",
                            "GPU",
                            enc == EncoderMode::Gpu,
                            |st| {
                                st.set_encoder(EncoderMode::Gpu);
                            },
                        ),
                        segmented_item(
                            p,
                            "settings-enc-cpu",
                            "CPU",
                            enc == EncoderMode::Cpu,
                            |st| {
                                st.set_encoder(EncoderMode::Cpu);
                            },
                        ),
                    ],
                ),
            ),
        },
        SettingRowSpec {
            title: "Video codec",
            description: "AV1 falls back automatically when unsupported by GPU/driver",
            category: "Recording Defaults Codec",
            element: settings_row(
                p,
                "row-video-codec",
                "Video codec",
                "AV1 falls back automatically when unsupported by GPU/driver",
                row()
                    .key("vcodec-opts")
                    .gap(space::XS)
                    .child(pill_option(
                        p,
                        "settings-vcodec-auto",
                        "Auto",
                        codec == VideoCodec::Auto,
                        |st| st.set_video_codec(VideoCodec::Auto),
                    ))
                    .child(pill_option(
                        p,
                        "settings-vcodec-h264",
                        "H.264",
                        codec == VideoCodec::H264,
                        |st| st.set_video_codec(VideoCodec::H264),
                    ))
                    .child(pill_option(
                        p,
                        "settings-vcodec-hevc",
                        "HEVC",
                        codec == VideoCodec::Hevc,
                        |st| st.set_video_codec(VideoCodec::Hevc),
                    ))
                    .child(pill_option(
                        p,
                        "settings-vcodec-av1",
                        "AV1",
                        codec == VideoCodec::Av1,
                        |st| st.set_video_codec(VideoCodec::Av1),
                    )),
            ),
        },
        SettingRowSpec {
            title: "Quality preset",
            description: "Target bitrate and compression quality for GSR capture",
            category: "Recording Defaults Quality",
            element: settings_row(
                p,
                "row-quality",
                "Quality preset",
                "Target bitrate and compression quality for GSR capture",
                row()
                    .key("quality-opts")
                    .gap(space::XS)
                    .child(pill_option(
                        p,
                        "settings-qual-med",
                        "Medium",
                        quality == QualityPreset::Medium,
                        |st| st.config.capture.quality = QualityPreset::Medium,
                    ))
                    .child(pill_option(
                        p,
                        "settings-qual-high",
                        "High",
                        quality == QualityPreset::High,
                        |st| st.config.capture.quality = QualityPreset::High,
                    ))
                    .child(pill_option(
                        p,
                        "settings-qual-vhigh",
                        "Very High",
                        quality == QualityPreset::VeryHigh,
                        |st| st.config.capture.quality = QualityPreset::VeryHigh,
                    ))
                    .child(pill_option(
                        p,
                        "settings-qual-ultra",
                        "Ultra",
                        quality == QualityPreset::Ultra,
                        |st| st.config.capture.quality = QualityPreset::Ultra,
                    )),
            ),
        },
        SettingRowSpec {
            title: "Container format",
            description: "Output file wrapper for saved replays and manual recordings",
            category: "Recording Defaults Container",
            element: settings_row(
                p,
                "row-container",
                "Container format",
                "Output file wrapper for saved replays and manual recordings",
                segmented_control(
                    p,
                    "settings-container-seg",
                    vec![
                        segmented_item(
                            p,
                            "settings-cont-mp4",
                            "MP4",
                            container == ContainerFormat::Mp4,
                            |st| st.config.capture.container = ContainerFormat::Mp4,
                        ),
                        segmented_item(
                            p,
                            "settings-cont-mkv",
                            "MKV",
                            container == ContainerFormat::Mkv,
                            |st| st.config.capture.container = ContainerFormat::Mkv,
                        ),
                    ],
                ),
            ),
        },
    ]
}

fn audio_tab(p: &Palette, s: &AppState) -> El {
    let rows: Vec<El> = audio_rows(p, s).into_iter().map(|r| r.element).collect();

    let mut device_rows = Vec::new();
    for (idx, dev) in s.available_audio_devices.iter().enumerate() {
        let src = dev.id.clone();
        let active = s.config.capture.audio_sources.contains(&src);
        let label = format!("{} ({})", dev.label, dev.id);
        device_rows.push(settings_row(
            p,
            &format!("row-audio-dev-{idx}"),
            &label,
            "PipeWire / PulseAudio device discovered by gpu-screen-recorder",
            toggle_switch(
                p,
                &format!("settings-audio-dev-{idx}"),
                &dev.label,
                active,
                move |st| st.toggle_audio_source(src.clone()),
            ),
        ));
    }

    let devices_section = if device_rows.is_empty() {
        card(p, "settings-audio-devices-empty").child(
            row()
                .key("audio-discover-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    col()
                        .key("audio-discover-meta")
                        .gap(2.0)
                        .child(
                            text("No extra PipeWire devices probed yet")
                                .font(typography::BODY)
                                .color(p.text_primary),
                        )
                        .child(
                            text("Probe available desktop sinks, microphones, and app streams via GSR.")
                                .font(typography::LABEL)
                                .color(p.text_muted),
                        ),
                )
                .child(btn_secondary(
                    p,
                    "settings-refresh-audio",
                    "Refresh Sources",
                    |st| st.discover_capture_sources(),
                )),
        )
    } else {
        col()
            .key("settings-audio-devices-wrap")
            .w_full()
            .gap(space::SM)
            .child(
                row()
                    .key("audio-devices-actions")
                    .w_full()
                    .justify_end()
                    .child(btn_secondary(
                        p,
                        "settings-refresh-audio",
                        "Refresh Sources",
                        |st| st.discover_capture_sources(),
                    )),
            )
            .child(settings_group(
                p,
                "audio-discovered-group",
                "DISCOVERED DEVICES",
                device_rows,
            ))
    };

    col()
        .key("settings-audio-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "audio-core-group",
            "CODEC & DEFAULT SOURCES",
            rows,
        ))
        .child(devices_section)
}

fn audio_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    let acodec = s.config.capture.audio_codec;
    let has_output = s
        .config
        .capture
        .audio_sources
        .iter()
        .any(|src| src == "default_output");
    let has_input = s
        .config
        .capture
        .audio_sources
        .iter()
        .any(|src| src == "default_input");

    vec![
        SettingRowSpec {
            title: "Audio codec",
            description: "Audio compression format (FLAC falls back when incompatible with MP4)",
            category: "Audio Codec",
            element: settings_row(
                p,
                "row-audio-codec",
                "Audio codec",
                "Audio compression format (FLAC falls back when incompatible with MP4)",
                row()
                    .key("acodec-opts")
                    .gap(space::XS)
                    .child(pill_option(
                        p,
                        "settings-acodec-opus",
                        "Opus",
                        acodec == AudioCodec::Opus,
                        |st| st.set_audio_codec(AudioCodec::Opus),
                    ))
                    .child(pill_option(
                        p,
                        "settings-acodec-aac",
                        "AAC",
                        acodec == AudioCodec::Aac,
                        |st| st.set_audio_codec(AudioCodec::Aac),
                    ))
                    .child(pill_option(
                        p,
                        "settings-acodec-flac",
                        "FLAC",
                        acodec == AudioCodec::Flac,
                        |st| st.set_audio_codec(AudioCodec::Flac),
                    )),
            ),
        },
        SettingRowSpec {
            title: "Default desktop audio output",
            description: "Capture system playback audio (default_output)",
            category: "Audio Sources",
            element: settings_row(
                p,
                "row-audio-default-out",
                "Default desktop audio output",
                "Capture system playback audio (default_output)",
                toggle_switch(
                    p,
                    "settings-audio-default-out",
                    "Default desktop audio output",
                    has_output,
                    |st| st.toggle_audio_source("default_output".to_string()),
                ),
            ),
        },
        SettingRowSpec {
            title: "Default microphone input",
            description: "Capture primary microphone input (default_input)",
            category: "Audio Sources",
            element: settings_row(
                p,
                "row-audio-default-in",
                "Default microphone input",
                "Capture primary microphone input (default_input)",
                toggle_switch(
                    p,
                    "settings-audio-default-in",
                    "Default microphone input",
                    has_input,
                    |st| st.toggle_audio_source("default_input".to_string()),
                ),
            ),
        },
    ]
}

fn shortcuts_tab(p: &Palette, s: &AppState) -> El {
    let rows: Vec<El> = shortcut_rows(p, s).into_iter().map(|r| r.element).collect();

    let wayland_notice = s.is_wayland.then(|| {
        row()
            .key("settings-wayland-hk-notice")
            .w_full()
            .px(space::MD)
            .py(space::SM)
            .gap(space::SM)
            .center_y()
            .radius(radius::MD)
            .fill(p.warning_soft)
            .border(1.0, p.warning)
            .child(badge(
                "wayland-hk-badge",
                "WAYLAND",
                p.warning_text,
                p.bg_lowest,
            ))
            .child(
                text(
                    "Global hotkeys are X11-only in this build. On Wayland, use your compositor shortcuts or the top bar / Recording Studio controls.",
                )
                .font(typography::LABEL)
                .color(p.text_primary),
            )
    });

    col()
        .key("settings-shortcuts-tab")
        .w_full()
        .gap(space::LG)
        .children(wayland_notice)
        .child(settings_group(
            p,
            "shortcuts-bindings-group",
            "CAPTURE HOTKEY BINDINGS",
            rows,
        ))
}

fn shortcut_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    vec![
        SettingRowSpec {
            title: "Save replay buffer",
            description: "Save the rolling replay buffer to disk immediately",
            category: "Keyboard Shortcuts Hotkeys",
            element: hotkey_binding_row(
                p,
                s,
                "save-replay",
                "Save replay buffer",
                "Save the rolling replay buffer to disk immediately",
                &s.hotkeys.save_replay,
                ["F8", "F9", "F10"],
                |st, key| st.hotkeys.save_replay = key.to_string(),
            ),
        },
        SettingRowSpec {
            title: "Toggle replay buffer",
            description: "Start or stop the background GPU Screen Recorder replay buffer",
            category: "Keyboard Shortcuts Hotkeys",
            element: hotkey_binding_row(
                p,
                s,
                "toggle-replay",
                "Toggle replay buffer",
                "Start or stop the background GPU Screen Recorder replay buffer",
                &s.hotkeys.toggle_replay,
                ["F9", "F10", "F11"],
                |st, key| st.hotkeys.toggle_replay = key.to_string(),
            ),
        },
        SettingRowSpec {
            title: "Start / stop manual recording",
            description: "Begin or finish a continuous manual recording session",
            category: "Keyboard Shortcuts Hotkeys",
            element: hotkey_binding_row(
                p,
                s,
                "toggle-record",
                "Start / stop manual recording",
                "Begin or finish a continuous manual recording session",
                &s.hotkeys.toggle_record,
                ["F10", "F11", "F12"],
                |st, key| st.hotkeys.toggle_record = key.to_string(),
            ),
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn hotkey_binding_row(
    p: &Palette,
    s: &AppState,
    key_suffix: &str,
    title: &str,
    description: &str,
    current: &str,
    choices: [&'static str; 3],
    setter: fn(&mut AppState, &'static str),
) -> El {
    let control = if s.is_wayland {
        row()
            .key(format!("hk-{key_suffix}-readonly"))
            .gap(space::SM)
            .center_y()
            .child(badge(
                &format!("hk-{key_suffix}-cap"),
                current,
                p.text_disabled,
                p.bg_lowest,
            ))
            .child(
                text("X11 only")
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            )
    } else {
        row()
            .key(format!("hk-{key_suffix}-opts"))
            .gap(space::XS)
            .children(choices.into_iter().map(|choice| {
                pill_option(
                    p,
                    &format!("settings-hk-{key_suffix}-{choice}"),
                    choice,
                    current == choice,
                    move |st| setter(st, choice),
                )
            }))
    };

    settings_row(
        p,
        &format!("row-hk-{key_suffix}"),
        title,
        description,
        control,
    )
}

fn appearance_tab(p: &Palette, s: &AppState) -> El {
    let rows: Vec<El> = appearance_rows(p, s)
        .into_iter()
        .map(|r| r.element)
        .collect();
    col()
        .key("settings-appearance-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "appearance-display-group",
            "DISPLAY & DENSITY",
            rows,
        ))
}

fn appearance_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    vec![
        SettingRowSpec {
            title: "Interface density",
            description: "Adjust padding and row compression across library and settings panels",
            category: "Appearance Layout",
            element: settings_row(
                p,
                "row-compact-density",
                "Interface density",
                "Adjust padding and row compression across library and settings panels",
                segmented_control(
                    p,
                    "settings-density-seg",
                    vec![
                        segmented_item(
                            p,
                            "settings-density-comfortable",
                            "Comfortable",
                            !s.config.compact_density,
                            |st| st.config.compact_density = false,
                        ),
                        segmented_item(
                            p,
                            "settings-density-compact",
                            "Compact",
                            s.config.compact_density,
                            |st| st.config.compact_density = true,
                        ),
                    ],
                ),
            ),
        },
        SettingRowSpec {
            title: "Color theme",
            description: "ClipForge Desktop uses the Obsidian Forge dark palette",
            category: "Appearance Theme",
            element: settings_row(
                p,
                "row-theme-info",
                "Color theme",
                "ClipForge Desktop uses the Obsidian Forge dark palette",
                badge(
                    "settings-theme-badge",
                    "Obsidian Forge Dark",
                    p.accent_primary,
                    p.accent_soft,
                ),
            ),
        },
    ]
}

fn sharing_tab(p: &Palette, s: &AppState) -> El {
    let rows: Vec<El> = sharing_rows(p, s).into_iter().map(|r| r.element).collect();

    let status_dot_color = if s.server_connected {
        p.success
    } else if s.server_error.is_some() {
        p.recording
    } else {
        p.text_muted
    };
    let status_label = if s.server_connected {
        "Connected"
    } else if s.server_health_ok == Some(true) {
        "Server Healthy"
    } else {
        "Disconnected"
    };

    let status_card = card(p, "settings-sharing-status-card")
        .gap(space::SM)
        .child(
            row()
                .key("sharing-status-hdr")
                .w_full()
                .justify_between()
                .center_y()
                .child(status_pill(
                    p,
                    "settings-server-pill",
                    status_dot_color,
                    status_label,
                    p.text_primary,
                    p.bg_lowest,
                    None,
                ))
                .child(
                    row()
                        .key("sharing-status-actions")
                        .gap(space::SM)
                        .child(btn_secondary(
                            p,
                            "settings-test-server",
                            "Test Connection",
                            |st| st.check_server_health(),
                        ))
                        .child(btn_secondary(
                            p,
                            "settings-open-cloud",
                            "Open Shared & Cloud",
                            |st| st.nav = NavSection::Uploads,
                        )),
                ),
        )
        .children(s.server_error.as_ref().map(|err| {
            text(err.clone())
                .font(typography::LABEL)
                .color(p.error_text)
        }));

    col()
        .key("settings-sharing-tab")
        .w_full()
        .gap(space::LG)
        .child(settings_group(
            p,
            "sharing-server-group",
            "SERVER ENDPOINT & AUTH",
            rows,
        ))
        .child(status_card)
}

fn sharing_rows(p: &Palette, s: &AppState) -> Vec<SettingRowSpec> {
    vec![
        SettingRowSpec {
            title: "Server endpoint URL",
            description: "Base URL of your self-hosted ClipForge server",
            category: "Sharing & Server",
            element: settings_row(
                p,
                "row-server-url",
                "Server endpoint URL",
                "Base URL of your self-hosted ClipForge server",
                el().key("server-url-wrap").w(280.0).child(text_input_box(
                    p,
                    "settings-server-url",
                    &s.server_url_edit,
                    "https://clips.example.com",
                )),
            ),
        },
        SettingRowSpec {
            title: "API bearer token",
            description: "Optional bearer token persisted to config.json for authenticated uploads",
            category: "Sharing & Server",
            element: settings_row(
                p,
                "row-api-token",
                "API bearer token",
                "Optional bearer token persisted to config.json for authenticated uploads",
                el().key("api-token-wrap").w(280.0).child(text_input_box(
                    p,
                    "settings-api-token",
                    &s.api_token_edit,
                    "Bearer token (optional)",
                )),
            ),
        },
    ]
}

fn about_tab(p: &Palette, s: &AppState) -> El {
    let sys_card = card(p, "settings-about-sys-card")
        .gap(space::XS)
        .child(kv_row(
            p,
            "about-version",
            "ClipForge Version",
            env!("CARGO_PKG_VERSION").to_string(),
        ))
        .child(kv_row(
            p,
            "about-session",
            "Display Session",
            if s.is_wayland { "Wayland" } else { "X11" }.to_string(),
        ))
        .child(kv_row(
            p,
            "about-config-path",
            "Config File",
            config_file_path().display().to_string(),
        ));

    let deps_card = card(p, "settings-deps-card")
        .child(
            row()
                .key("deps-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "deps-hdr",
                    "System Dependencies",
                    "External tools used for GPU capture and media processing",
                ))
                .child(btn_secondary(p, "deps-recheck", "Re-check", |st| {
                    if let Some(w) = &st.worker {
                        w.send(WorkerCommand::ProbeDependencies);
                    }
                })),
        )
        .child(dep_row(
            p,
            "dep-gsr",
            "gpu-screen-recorder",
            s.deps.gsr_available,
            "Install via Flatpak (com.dec05eba.gpu_screen_recorder) or distro package",
        ))
        .child(dep_row(
            p,
            "dep-ffmpeg",
            "ffmpeg",
            s.deps.ffmpeg_available,
            "sudo apt install ffmpeg",
        ))
        .child(dep_row(
            p,
            "dep-ffprobe",
            "ffprobe",
            s.deps.ffprobe_available,
            "Included with ffmpeg package",
        ));

    col()
        .key("settings-about-tab")
        .w_full()
        .gap(space::LG)
        .child(sys_card)
        .child(deps_card)
}

fn disabled_pill(p: &Palette, key: &str, label_text: &str) -> El {
    row()
        .key(key)
        .h(30.0)
        .px(space::MD)
        .center_y()
        .radius(radius::PILL)
        .fill(p.bg_lowest)
        .border(1.0, p.border_subtle)
        .child(
            text(label_text.to_string())
                .font(typography::LABEL)
                .color(p.text_disabled),
        )
}

fn dep_row(p: &Palette, key: &str, name: &str, ok: bool, guidance: &str) -> El {
    row()
        .key(key)
        .w_full()
        .py(space::XS)
        .justify_between()
        .center_y()
        .child(
            col()
                .key("info")
                .gap(2.0)
                .child(
                    text(name.to_string())
                        .font(typography::BODY)
                        .color(p.text_primary),
                )
                .child(
                    text(guidance.to_string())
                        .font(typography::LABEL)
                        .color(p.text_muted),
                ),
        )
        .child(if ok {
            badge("ok", "Installed", p.success_text, p.success_soft)
        } else {
            badge("missing", "Missing", p.warning_text, p.warning_soft)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NavSection;
    use crate::views::root_view;
    use rune::{Env, Runtime};

    fn click_key(rt: &mut Runtime<AppState>, key: &str) {
        let (cx, cy) = rt
            .find(key)
            .unwrap_or_else(|| panic!("missing key {key} in UI tree"));
        rt.pointer_down(cx, cy);
        rt.pointer_up(cx, cy);
        rt.frame(0.0);
    }

    #[test]
    fn settings_tabs_and_search_render_and_filter() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(
            AppState {
                nav: NavSection::Settings,
                ..AppState::default()
            },
            root_view,
            (1240.0, 800.0),
            env,
        );
        rt.frame(0.0);

        for tab in SettingsTab::ALL {
            click_key(&mut rt, tab.key());
            assert_eq!(rt.state.settings_tab, tab);
        }

        let mut s = AppState {
            nav: NavSection::Settings,
            settings_search: rune::TextEdit::new("codec".to_string()),
            ..AppState::default()
        };
        let mut rt_search = Runtime::new(
            std::mem::take(&mut s),
            root_view,
            (1240.0, 800.0),
            Env::new(true, 1.0),
        );
        rt_search.frame(0.0);
        assert!(rt_search.find("settings-search-group").is_some());
        assert!(rt_search.find("row-video-codec").is_some());
        assert!(rt_search.find("row-start-minimized").is_none());

        let rt_empty = {
            let mut r = Runtime::new(
                AppState {
                    nav: NavSection::Settings,
                    settings_search: rune::TextEdit::new("zzzz-no-match".to_string()),
                    ..AppState::default()
                },
                root_view,
                (1240.0, 800.0),
                Env::new(true, 1.0),
            );
            r.frame(0.0);
            r
        };
        assert!(rt_empty.find("settings-search-empty").is_some());
    }

    #[test]
    fn settings_wayland_disables_window_target_and_shows_hotkey_notice() {
        let env = Env::new(true, 1.0);
        let mut rt = Runtime::new(
            AppState {
                nav: NavSection::Settings,
                is_wayland: true,
                ..AppState::default()
            },
            root_view,
            (1240.0, 800.0),
            env,
        );
        rt.frame(0.0);

        click_key(&mut rt, SettingsTab::Recording.key());
        assert!(rt.find("settings-target-window-disabled").is_some());
        assert!(rt.find("settings-target-window").is_none());

        click_key(&mut rt, SettingsTab::Shortcuts.key());
        assert!(rt.find("settings-wayland-hk-notice").is_some());
    }
}
