//! Settings view: `AppConfig` persistence, paths, server URL, and Linux dependency status.

use clipforge_common::EncoderMode;
use rune::tok::type_;
use rune::{El, col, row, text};

use crate::components::{
    badge, btn_primary, card, kv_row, pill_option, section_header, toggle_switch,
};
use crate::config::config_file_path;
use crate::state::AppState;
use crate::theme::{Palette, space};
use crate::worker::WorkerCommand;

pub fn view(p: &Palette, s: &AppState) -> El {
    let config_card = card(p, "settings-config-card")
        .child(
            row()
                .key("settings-hdr-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(section_header(
                    p,
                    "settings-hdr",
                    "Application Configuration",
                    "Persisted atomically to config.json",
                ))
                .child(btn_primary(p, "settings-save", "Save Settings", |st| {
                    st.save_config();
                })),
        )
        .child(kv_row(
            p,
            "cfg-path",
            "Config File",
            config_file_path().display().to_string(),
        ))
        .child(kv_row(
            p,
            "cfg-clips-dir",
            "Clips Directory",
            s.config.clips_dir.display().to_string(),
        ))
        .child(kv_row(
            p,
            "cfg-db-path",
            "SQLite Library DB",
            s.config.database_path.display().to_string(),
        ))
        .child(kv_row(
            p,
            "cfg-server",
            "Cloud Server URL",
            s.config
                .server_url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:8080 (unset)".to_string()),
        ))
        .child(
            row()
                .key("cfg-encoder-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Default Encoder")
                        .font(type_::BODY)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("cfg-enc-opts")
                        .gap(space::SM)
                        .child(pill_option(
                            p,
                            "settings-enc-gpu",
                            "GPU",
                            s.config.capture.encoder == EncoderMode::Gpu,
                            |st| st.set_encoder(EncoderMode::Gpu),
                        ))
                        .child(pill_option(
                            p,
                            "settings-enc-cpu",
                            "CPU",
                            s.config.capture.encoder == EncoderMode::Cpu,
                            |st| st.set_encoder(EncoderMode::Cpu),
                        )),
                ),
        )
        .child(
            row()
                .key("cfg-thumbs-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Auto-generate Thumbnails")
                        .font(type_::BODY)
                        .color(p.text_secondary),
                )
                .child(toggle_switch(
                    p,
                    "settings-thumbs-switch",
                    "Auto-generate Thumbnails",
                    s.config.auto_generate_thumbnails,
                    |st| st.config.auto_generate_thumbnails = !st.config.auto_generate_thumbnails,
                )),
        );

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
                .child(btn_primary(p, "deps-recheck", "Re-check", |st| {
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
        .key("settings-view")
        .w_full()
        .gap(space::LG)
        .child(config_card)
        .child(deps_card)
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
                        .font(type_::BODY)
                        .color(p.text_primary),
                )
                .child(
                    text(guidance.to_string())
                        .font(type_::LABEL)
                        .color(p.text_muted),
                ),
        )
        .child(if ok {
            badge("ok", "Installed", p.success, p.success_soft)
        } else {
            badge("missing", "Missing", p.warning, p.warning_soft)
        })
}
