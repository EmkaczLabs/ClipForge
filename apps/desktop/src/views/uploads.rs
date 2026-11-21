//! Shared & Cloud view: server connection setup, shared clip grid, upload queue, and clip inspector.

use clipforge_cloud_client::{RemoteClipSummary, ShareVisibility};
use clipforge_common::ClipId;
use rune::{Cursor, El, SurfaceFit, col, el, icon, row, text};

use crate::components::{
    badge, btn_danger, btn_ghost, btn_primary, btn_secondary, card, empty_state, icon_btn_sized,
    icons, kv_row, panel, pill_option, progress_bar, secure_input_box, status_pill, text_input_box,
};
use crate::state::{
    AppState, NavSection, UploadEntry, UploadStatus, format_bytes, format_duration_ms,
};
use crate::theme::{Palette, radius, snappy_spring, space, typography};
use crate::worker::WorkerCommand;

/// Renders the Shared & Cloud workspace in either Disconnected or Connected mode.
pub fn view(p: &Palette, s: &AppState) -> El {
    if !s.server_connected {
        disconnected_view(p, s)
    } else {
        connected_view(p, s)
    }
}

fn disconnected_view(p: &Palette, s: &AppState) -> El {
    let (pill_dot, pill_label, pill_fg, pill_bg) = match s.server_health_ok {
        Some(true) => (
            p.warning,
            "Reachable • Not Authenticated",
            p.warning_text,
            p.warning_soft,
        ),
        Some(false) => (
            p.recording,
            "Unreachable • Connection Failed",
            p.error_text,
            p.recording_soft,
        ),
        None => (
            p.warning,
            "Offline • No Server Connected",
            p.text_secondary,
            p.bg_surface,
        ),
    };

    let header = row()
        .key("uploads-disconnected-hdr")
        .w_full()
        .justify_between()
        .center_y()
        .child(
            col()
                .key("hdr-titles")
                .gap(2.0)
                .child(
                    text("WORKSPACE // REMOTE DISTRIBUTION • Standalone Mode")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(
                    text("Shared & Cloud")
                        .font(typography::HEADLINE_LG)
                        .color(p.text_primary),
                ),
        )
        .child(status_pill(
            p,
            "uploads-server-badge",
            pill_dot,
            pill_label,
            pill_fg,
            pill_bg,
            None,
        ));

    let feedback = if let Some(err) = &s.server_error {
        Some(
            row()
                .key("server-error-banner")
                .w_full()
                .padding(space::SM)
                .gap(space::SM)
                .center_y()
                .radius(radius::SM)
                .fill(p.recording_soft)
                .border(1.0, p.recording)
                .child(icon(&icons::WARNING, 15.0).color(p.recording).a11y_hidden())
                .child(
                    text(err.clone())
                        .font(typography::LABEL)
                        .color(p.error_text)
                        .grow(1.0)
                        .min_w(0.0),
                ),
        )
    } else if s.server_health_ok == Some(true) {
        let ver_note = s
            .server_info
            .as_ref()
            .map(|i| format!(" (v{}, {})", i.version, i.storage_backend))
            .unwrap_or_default();
        Some(
            row()
                .key("server-health-banner")
                .w_full()
                .padding(space::SM)
                .gap(space::SM)
                .center_y()
                .radius(radius::SM)
                .fill(p.success_soft)
                .border(1.0, p.success)
                .child(icon(&icons::CHECK, 15.0).color(p.success).a11y_hidden())
                .child(
                    text(format!(
                        "Server /health check passed{ver_note}. Click Connect Server to authenticate and sync."
                    ))
                    .font(typography::LABEL)
                    .color(p.success_text)
                    .grow(1.0)
                    .min_w(0.0),
                ),
        )
    } else {
        None
    };

    let connect_card = card(p, "connect-server-card")
        .w(600.0)
        .gap(space::MD)
        .child(
            col()
                .key("connect-hero")
                .w_full()
                .center()
                .gap(space::SM)
                .child(
                    row()
                        .key("connect-icon-box")
                        .size(44.0, 44.0)
                        .center()
                        .radius(radius::MD)
                        .fill(p.accent_soft)
                        .border(1.0, p.accent_brand)
                        .child(
                            icon(&icons::CLOUD, 22.0)
                                .color(p.accent_primary)
                                .a11y_hidden(),
                        ),
                )
                .child(
                    text("Connect Self-Hosted Server")
                        .font(typography::HEADLINE_MD)
                        .color(p.text_primary),
                )
                .child(
                    text("Connect your own server to share clips. Local recording & editing work offline.")
                        .font(typography::BODY)
                        .color(p.text_secondary),
                ),
        )
        .child(
            col()
                .key("server-url-group")
                .w_full()
                .gap(space::XS)
                .child(
                    row()
                        .key("server-url-lbl")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text("Server Endpoint URL")
                                .font(typography::LABEL)
                                .color(p.text_primary),
                        )
                        .child(
                            text("HTTP/2 or HTTPS")
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        ),
                )
                .child(text_input_box(
                    p,
                    "server-url-input",
                    &s.server_url_edit,
                    "https://clips.example.com:9443",
                    |st| &mut st.server_url_edit,
                )),
        )
        .child(
            col()
                .key("server-token-group")
                .w_full()
                .gap(space::XS)
                .child(
                    row()
                        .key("server-token-lbl")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text("API Bearer Token (Optional)")
                                .font(typography::LABEL)
                                .color(p.text_primary),
                        )
                        .child(
                            text("token / bearer")
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        ),
                )
                .child(secure_input_box(
                    p,
                    "server-token-input",
                    &s.api_token_edit,
                    "cf_tok_...",
                    |st| &mut st.api_token_edit,
                )),
        )
        .child(
            row()
                .key("server-auth-row")
                .w_full()
                .gap(space::MD)
                .child(
                    col()
                        .key("server-user-col")
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(space::XS)
                        .child(
                            text("Username (Optional)")
                                .font(typography::LABEL)
                                .color(p.text_secondary),
                        )
                        .child(text_input_box(
                            p,
                            "server-user-input",
                            &s.server_user_edit,
                            "username",
                            |st| &mut st.server_user_edit,
                        )),
                )
                .child(
                    col()
                        .key("server-pass-col")
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(space::XS)
                        .child(
                            text("Password (Optional)")
                                .font(typography::LABEL)
                                .color(p.text_secondary),
                        )
                        .child(secure_input_box(
                            p,
                            "server-pass-input",
                            &s.server_pass_edit,
                            "password",
                            |st| &mut st.server_pass_edit,
                        )),
                ),
        )
        .children(feedback)
        .child(
            row()
                .key("connect-footer")
                .w_full()
                .pt(space::XS)
                .justify_between()
                .center_y()
                .border_sides([1.0, 0.0, 0.0, 0.0], p.border_subtle)
                .child(btn_secondary(
                    p,
                    "test-connection-btn",
                    "Test Connection",
                    |st| st.check_server_health(),
                ))
                .child(
                    row()
                        .key("connect-actions")
                        .gap(space::SM)
                        .center_y()
                        .child(btn_ghost(
                            p,
                            "register-server-btn",
                            "Register Account",
                            |st| st.connect_server(true),
                        ))
                        .child(btn_primary(
                            p,
                            "connect-server-btn",
                            "Connect Server",
                            |st| st.connect_server(false),
                        )),
                ),
        );

    let guide_card = card(p, "self-host-guide-card")
        .w(600.0)
        .gap(space::SM)
        .child(
            text("Self-Hosting Guide")
                .font(typography::HEADLINE_SM)
                .color(p.text_primary),
        )
        .child(guide_step(
            p,
            "guide-1",
            "1",
            "Start clipforge-server on your LAN, NAS, or VPS (cargo run -p clipforge-server).",
        ))
        .child(guide_step(
            p,
            "guide-2",
            "2",
            "Enter the base URL above (for example http://127.0.0.1:9443 or https://clips.lan.home:9443).",
        ))
        .child(guide_step(
            p,
            "guide-3",
            "3",
            "Sign in with an existing API token or enter a username & password to login or register.",
        ));

    let local_card = card(p, "local-status-card").w(600.0).child(
        row()
            .key("local-status-row")
            .w_full()
            .justify_between()
            .center_y()
            .gap(space::MD)
            .child(
                row()
                    .key("local-status-info")
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(space::SM)
                    .center_y()
                    .child(
                        el().key("local-dot")
                            .size(8.0, 8.0)
                            .radius(4.0)
                            .fill(p.success)
                            .a11y_hidden(),
                    )
                    .child(
                        col()
                            .key("local-copy")
                            .grow(1.0)
                            .min_w(0.0)
                            .gap(2.0)
                            .child(
                                text("Local Recording & Library Available")
                                    .font(typography::LABEL)
                                    .color(p.text_primary),
                            )
                            .child(
                                text(format!(
                                    "Storing clips in {}. Replay Buffer, Recording, and Editor work offline.",
                                    s.config.clips_dir.display()
                                ))
                                .font(typography::MICRO)
                                .color(p.text_secondary),
                            ),
                    ),
            )
            .child(btn_secondary(
                p,
                "go-library-btn",
                "Go to Clip Library",
                |st| st.nav = NavSection::Library,
            )),
    );

    col()
        .key("uploads-view")
        .w_full()
        .gap(space::LG)
        .child(header)
        .child(
            col()
                .key("uploads-disconnected-center")
                .w_full()
                .center()
                .gap(space::LG)
                .child(connect_card)
                .child(guide_card)
                .child(local_card),
        )
}

fn guide_step(p: &Palette, key: &str, num: &str, copy: &str) -> El {
    row()
        .key(key)
        .w_full()
        .gap(space::SM)
        .center_y()
        .child(badge("step-num", num, p.accent_primary, p.accent_soft))
        .child(
            text(copy.to_string())
                .font(typography::LABEL)
                .color(p.text_secondary)
                .grow(1.0)
                .min_w(0.0),
        )
}

#[derive(Clone)]
struct SharedGridItem<'a> {
    clip_id: ClipId,
    title: String,
    game_name: Option<String>,
    upload: Option<&'a UploadEntry>,
    remote: Option<&'a RemoteClipSummary>,
}

impl<'a> SharedGridItem<'a> {
    fn share_id(&self) -> Option<&str> {
        self.remote
            .map(|r| r.share_id.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.upload
                    .and_then(|u| u.share.as_ref().map(|s| s.share_id.as_str()))
                    .filter(|s| !s.is_empty())
            })
    }

    fn share_url(&self) -> Option<&str> {
        self.remote
            .map(|r| r.url.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.upload
                    .and_then(|u| u.share.as_ref().map(|s| s.url.as_str()))
                    .filter(|s| !s.is_empty())
            })
    }

    fn is_public(&self) -> bool {
        if let Some(u) = self.upload {
            u.is_public
        } else if let Some(r) = self.remote {
            r.visibility == ShareVisibility::Public
        } else {
            false
        }
    }
}

fn collect_shared_items<'a>(s: &'a AppState) -> Vec<SharedGridItem<'a>> {
    let mut items: Vec<SharedGridItem<'a>> = Vec::new();

    for upload in &s.uploads {
        let remote = s.remote_shares.iter().find(|r| {
            r.clip_id == upload.clip_id
                || upload
                    .share
                    .as_ref()
                    .is_some_and(|sh| sh.share_id == r.share_id)
        });
        items.push(SharedGridItem {
            clip_id: upload.clip_id,
            title: upload.title.clone(),
            game_name: Some(upload.game_name.clone()),
            upload: Some(upload),
            remote,
        });
    }

    for remote in &s.remote_shares {
        let already_present = items.iter().any(|it| {
            it.clip_id == remote.clip_id || it.share_id() == Some(remote.share_id.as_str())
        });
        if !already_present {
            let local_game = s
                .clips
                .iter()
                .find(|c| c.id == remote.clip_id)
                .and_then(|c| c.game.as_ref().map(|g| g.name.clone()));
            items.push(SharedGridItem {
                clip_id: remote.clip_id,
                title: remote.title.clone(),
                game_name: local_game,
                upload: None,
                remote: Some(remote),
            });
        }
    }

    items
}

fn connected_view(p: &Palette, s: &AppState) -> El {
    let server_label = s
        .config
        .server_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .unwrap_or("Configured Server");

    let server_sub = match (&s.server_info, &s.current_user) {
        (Some(info), Some(user)) => format!(
            "{} v{} ({}) • Signed in as {}",
            info.name, info.version, info.storage_backend, user.username
        ),
        (Some(info), None) => format!(
            "{} v{} ({}) • Remote Shares & Upload Queue",
            info.name, info.version, info.storage_backend
        ),
        (None, Some(user)) => format!("Self-Hosted Vault • Signed in as {}", user.username),
        (None, None) => "Self-Hosted Vault • Remote Shares & Upload Queue".to_string(),
    };

    let header = row()
        .key("uploads-connected-hdr")
        .w_full()
        .justify_between()
        .center_y()
        .gap(space::MD)
        .child(
            col()
                .key("hdr-left")
                .gap(2.0)
                .child(
                    text("Shared & Cloud")
                        .font(typography::HEADLINE_LG)
                        .color(p.text_primary),
                )
                .child(
                    text(server_sub)
                        .font(typography::LABEL)
                        .color(p.text_secondary),
                ),
        )
        .child(
            row()
                .key("hdr-right")
                .gap(space::SM)
                .center_y()
                .child(status_pill(
                    p,
                    "uploads-server-badge",
                    p.success,
                    &format!("CONNECTED • {server_label}"),
                    p.success_text,
                    p.success_soft,
                    None,
                ))
                .child(btn_secondary(p, "refresh-shares-btn", "Refresh", |st| {
                    st.refresh_remote_shares()
                }))
                .child(btn_secondary(
                    p,
                    "disconnect-server-btn",
                    "Disconnect",
                    |st| st.disconnect_server(),
                ))
                .child(btn_primary(p, "upload-clips-btn", "+ Upload Clips", |st| {
                    st.nav = NavSection::Library;
                    st.status_banner =
                        Some("Select a clip in Clip Library and click Share to upload".to_string());
                })),
        );

    let error_banner = s.server_error.as_ref().map(|err| {
        row()
            .key("connected-error-banner")
            .w_full()
            .padding(space::SM)
            .gap(space::SM)
            .center_y()
            .justify_between()
            .radius(radius::MD)
            .fill(p.recording_soft)
            .border(1.0, p.recording)
            .child(
                row()
                    .key("err-msg")
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(space::SM)
                    .center_y()
                    .child(icon(&icons::WARNING, 15.0).color(p.recording).a11y_hidden())
                    .child(
                        text(err.clone())
                            .font(typography::LABEL)
                            .color(p.error_text)
                            .grow(1.0)
                            .min_w(0.0),
                    ),
            )
            .child(btn_secondary(p, "retry-refresh-btn", "Retry", |st| {
                st.refresh_remote_shares();
            }))
    });

    let active_uploads = s
        .uploads
        .iter()
        .filter(|u| {
            matches!(
                u.status,
                UploadStatus::Queued | UploadStatus::Uploading { .. }
            )
        })
        .count();
    let failed_uploads = s
        .uploads
        .iter()
        .filter(|u| matches!(u.status, UploadStatus::Failed(_)))
        .count();

    let summary_bar = row()
        .key("uploads-summary-bar")
        .w_full()
        .gap(space::SM)
        .center_y()
        .child(badge(
            "summary-local",
            &format!("Local Queue: {}", s.uploads.len()),
            p.text_primary,
            p.bg_surface,
        ))
        .child(badge(
            "summary-remote",
            &format!("Remote Shares: {}", s.remote_shares.len()),
            p.accent_text,
            p.accent_soft,
        ))
        .child(badge(
            "summary-active",
            &format!("Active: {active_uploads}"),
            if active_uploads > 0 {
                p.warning_text
            } else {
                p.text_secondary
            },
            if active_uploads > 0 {
                p.warning_soft
            } else {
                p.bg_surface
            },
        ))
        .child(badge(
            "summary-failed",
            &format!("Failed: {failed_uploads}"),
            if failed_uploads > 0 {
                p.error_text
            } else {
                p.text_secondary
            },
            if failed_uploads > 0 {
                p.recording_soft
            } else {
                p.bg_surface
            },
        ))
        .children(s.current_user.as_ref().map(|u| {
            badge(
                "summary-user",
                &format!("User: {}", u.username),
                p.text_primary,
                p.bg_surface,
            )
        }))
        .children(s.server_info.as_ref().map(|info| {
            badge(
                "summary-version",
                &format!("Server v{}", info.version),
                p.text_secondary,
                p.bg_surface,
            )
        }));

    let quota_card = s.quota.map(|q| {
        let ratio = if q.limit_bytes == 0 {
            0.0
        } else {
            (q.used_bytes as f32 / q.limit_bytes as f32).clamp(0.0, 1.0)
        };
        let bar_color = if ratio >= 0.9 {
            p.recording
        } else if ratio >= 0.75 {
            p.warning
        } else {
            p.accent_primary
        };
        card(p, "cloud-quota-card")
            .w_full()
            .gap(space::XS)
            .child(
                row()
                    .key("quota-hdr")
                    .w_full()
                    .justify_between()
                    .center_y()
                    .child(
                        text("Cloud Storage Quota")
                            .font(typography::LABEL)
                            .color(p.text_primary),
                    )
                    .child(
                        text(format!(
                            "{} / {} ({} clips • {} remaining)",
                            format_bytes(q.used_bytes),
                            format_bytes(q.limit_bytes),
                            q.clip_count,
                            format_bytes(q.remaining_bytes),
                        ))
                        .font(typography::MONO_METRIC)
                        .color(p.text_secondary),
                    ),
            )
            .child(progress_bar(p, "quota-bar", ratio, bar_color))
    });

    let items = collect_shared_items(s);

    let grid_section = if items.is_empty() {
        empty_state(
            p,
            "uploads-empty",
            "No shared clips or uploads yet",
            "Upload a clip from your Clip Library or refresh to load remote shares from your server.",
            Some(btn_primary(
                p,
                "empty-go-library",
                "Browse Clip Library",
                |st| {
                    st.nav = NavSection::Library;
                },
            )),
        )
    } else {
        let mut grid = col().key("shared-clips-grid").w_full().gap(space::MD);
        for (row_idx, chunk) in items.chunks(2).enumerate() {
            let mut grid_row = row()
                .key(format!("shared-grid-row-{row_idx}"))
                .w_full()
                .gap(space::MD);
            for item in chunk {
                grid_row = grid_row.child(shared_clip_card(p, s, item).grow(1.0).min_w(0.0));
            }
            if chunk.len() == 1 {
                grid_row = grid_row.child(el().key("grid-spacer").grow(1.0).min_w(0.0));
            }
            grid = grid.child(grid_row);
        }
        grid
    };

    let queue_section = transfer_queue_card(p, s);

    let left_col = col()
        .key("uploads-main-col")
        .grow(1.0)
        .min_w(0.0)
        .gap(space::LG)
        .child(grid_section)
        .child(queue_section);

    let selected_item = s
        .selected_upload_id
        .and_then(|sel_id| items.iter().find(|it| it.clip_id == sel_id));

    let split = row()
        .key("connected-split")
        .w_full()
        .gap(space::LG)
        .child(left_col)
        .children(selected_item.map(|it| inspector_sidebar(p, s, it)));

    col()
        .key("uploads-view")
        .w_full()
        .gap(space::LG)
        .child(header)
        .children(error_banner)
        .child(summary_bar)
        .children(quota_card)
        .child(split)
}

fn shared_clip_card(p: &Palette, s: &AppState, item: &SharedGridItem<'_>) -> El {
    let clip_id = item.clip_id;
    let selected = s.selected_upload_id == Some(clip_id);
    let share_url = item.share_url().map(str::to_string);

    let (origin_label, origin_fg, origin_bg) = match (item.upload.is_some(), item.remote.is_some())
    {
        (true, true) => ("LOCAL + REMOTE", p.success_text, p.success_soft),
        (true, false) => ("LOCAL QUEUE", p.warning_text, p.warning_soft),
        (false, true) => ("REMOTE ONLY", p.accent_text, p.accent_soft),
        (false, false) => ("LOCAL", p.text_secondary, p.bg_elevated),
    };

    let vis_badge = if item.is_public() {
        badge("vis-badge", "PUBLIC", p.success_text, p.success_soft)
    } else {
        badge("vis-badge", "PRIVATE", p.text_secondary, p.bg_elevated)
    };

    let preview = if let Some(px) = s.library_thumbs.get(&clip_id) {
        el().key("card-thumb")
            .w_full()
            .h(132.0)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        let (preview_label, preview_color) = match item.upload.map(|u| &u.status) {
            Some(UploadStatus::Uploading { progress_pct }) => {
                (format!("Uploading {progress_pct}%"), p.accent_text)
            }
            Some(UploadStatus::Queued) => ("Pending upload".to_string(), p.warning_text),
            Some(UploadStatus::Failed(_)) => ("Upload failed".to_string(), p.error_text),
            Some(UploadStatus::Cancelled) => ("Upload cancelled".to_string(), p.text_muted),
            _ => ("Remote shared clip".to_string(), p.text_muted),
        };
        col()
            .key("card-thumb-placeholder")
            .w_full()
            .h(132.0)
            .center()
            .gap(space::XS)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .child(icon(&icons::CLOUD, 22.0).color(preview_color).a11y_hidden())
            .child(
                text(preview_label)
                    .font(typography::MONO_BADGE)
                    .color(preview_color),
            )
    };

    let status_block = match item.upload.map(|u| &u.status) {
        Some(UploadStatus::Uploading { progress_pct }) => Some(
            col()
                .key("card-progress")
                .w_full()
                .gap(space::XS)
                .child(progress_bar(
                    p,
                    "card-progress-bar",
                    f32::from(*progress_pct) / 100.0,
                    p.accent_primary,
                ))
                .child(
                    row()
                        .key("card-progress-meta")
                        .w_full()
                        .justify_between()
                        .center_y()
                        .child(
                            text(format!("Uploading • {progress_pct}%"))
                                .font(typography::MONO_METRIC)
                                .color(p.accent_text),
                        )
                        .child(btn_ghost(
                            p,
                            &format!("card-cancel-{clip_id}"),
                            "Cancel",
                            move |st| {
                                st.cancel_upload(clip_id);
                            },
                        )),
                ),
        ),
        Some(UploadStatus::Queued) => Some(
            row()
                .key("card-queued-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(badge("queued", "PENDING", p.warning_text, p.warning_soft))
                .child(btn_ghost(
                    p,
                    &format!("card-cancel-{clip_id}"),
                    "Cancel",
                    move |st| {
                        st.cancel_upload(clip_id);
                    },
                )),
        ),
        Some(UploadStatus::Cancelled) => Some(
            row()
                .key("card-cancelled-row")
                .w_full()
                .justify_between()
                .center_y()
                .child(badge(
                    "cancelled",
                    "CANCELLED",
                    p.text_secondary,
                    p.bg_elevated,
                ))
                .child(
                    row()
                        .key("card-cancelled-actions")
                        .gap(space::XS)
                        .child(btn_primary(
                            p,
                            &format!("card-retry-{clip_id}"),
                            "Retry",
                            move |st| st.retry_upload(clip_id),
                        ))
                        .child(btn_ghost(
                            p,
                            &format!("card-dismiss-{clip_id}"),
                            "Dismiss",
                            move |st| st.cancel_upload(clip_id),
                        )),
                ),
        ),
        Some(UploadStatus::Failed(err)) => Some(
            col()
                .key("card-failed-block")
                .w_full()
                .gap(space::XS)
                .child(
                    text(err.clone())
                        .font(typography::MICRO)
                        .color(p.error_text),
                )
                .child(
                    row()
                        .key("card-failed-actions")
                        .w_full()
                        .gap(space::SM)
                        .child(btn_primary(
                            p,
                            &format!("card-retry-{clip_id}"),
                            "Retry",
                            move |st| {
                                st.retry_upload(clip_id);
                            },
                        ))
                        .child(btn_secondary(
                            p,
                            &format!("card-cancel-{clip_id}"),
                            "Cancel",
                            move |st| {
                                st.cancel_upload(clip_id);
                            },
                        )),
                ),
        ),
        _ => None,
    };

    let link_row = share_url.map(|url| {
        let url_copy = url.clone();
        let url_open = url.clone();
        row()
            .key("card-link-row")
            .w_full()
            .px(space::SM)
            .py(space::XS)
            .gap(space::XS)
            .center_y()
            .justify_between()
            .radius(radius::XS)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .child(
                text(url)
                    .font(typography::MONO_METRIC)
                    .color(p.text_secondary)
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(btn_secondary(
                p,
                &format!("card-copy-{clip_id}"),
                "Copy",
                move |st| {
                    st.copy_to_clipboard(url_copy.clone());
                },
            ))
            .child(btn_ghost(
                p,
                &format!("card-open-{clip_id}"),
                "Open",
                move |st| {
                    st.open_url(url_open.clone());
                },
            ))
    });

    let meta_line = match (item.game_name.as_deref(), item.remote.map(|r| r.size_bytes)) {
        (Some(game), Some(bytes)) => format!("{game} • {}", format_bytes(bytes)),
        (Some(game), None) => game.to_string(),
        (None, Some(bytes)) => format_bytes(bytes),
        (None, None) => "Clip".to_string(),
    };

    col()
        .key(format!("shared-card-{clip_id}"))
        .role("button")
        .label(&item.title)
        .checked(selected)
        .focusable(true)
        .focus_ring(1.0, 1.0, p.border_focus)
        .cursor(Cursor::Pointer)
        .padding(space::MD)
        .gap(space::SM)
        .radius(radius::MD)
        .fill(if selected {
            p.bg_elevated
        } else {
            p.bg_surface
        })
        .border(
            1.0,
            if selected {
                p.border_focus
            } else {
                p.border_subtle
            },
        )
        .hover(p.bg_hover)
        .motion(snappy_spring())
        .on_press(move |st: &mut AppState| {
            st.selected_upload_id = Some(clip_id);
        })
        .child(
            row()
                .key("card-badges")
                .w_full()
                .justify_between()
                .center_y()
                .child(badge("origin-badge", origin_label, origin_fg, origin_bg))
                .child(vis_badge),
        )
        .child(preview)
        .child(
            col()
                .key("card-meta")
                .w_full()
                .gap(2.0)
                .child(
                    text(item.title.clone())
                        .font(typography::HEADLINE_SM)
                        .color(p.text_primary),
                )
                .child(
                    text(meta_line)
                        .font(typography::LABEL)
                        .color(p.text_secondary),
                ),
        )
        .children(status_block)
        .children(link_row)
}

fn transfer_queue_card(p: &Palette, s: &AppState) -> El {
    let mut rows = col().key("queue-rows").w_full().gap(space::SM);
    if s.uploads.is_empty() {
        rows = rows.child(
            text(
                "No local uploads in queue. Use + Upload Clips to select a clip from your library.",
            )
            .font(typography::LABEL)
            .color(p.text_muted),
        );
    } else {
        for (idx, entry) in s.uploads.iter().enumerate() {
            let clip_id = entry.clip_id;
            let (status_str, status_fg, status_bg) = match &entry.status {
                UploadStatus::Queued => ("Pending".to_string(), p.warning_text, p.warning_soft),
                UploadStatus::Uploading { progress_pct } => (
                    format!("Uploading {progress_pct}%"),
                    p.accent_text,
                    p.accent_soft,
                ),
                UploadStatus::Completed => {
                    ("Completed".to_string(), p.success_text, p.success_soft)
                }
                UploadStatus::Failed(_) => ("Failed".to_string(), p.error_text, p.recording_soft),
                UploadStatus::Cancelled => {
                    ("Cancelled".to_string(), p.text_secondary, p.bg_elevated)
                }
            };

            let progress = match &entry.status {
                UploadStatus::Uploading { progress_pct } => Some(progress_bar(
                    p,
                    &format!("q-prog-{idx}"),
                    f32::from(*progress_pct) / 100.0,
                    p.accent_primary,
                )),
                _ => None,
            };

            let err_note = match &entry.status {
                UploadStatus::Failed(msg) => Some(
                    text(msg.clone())
                        .font(typography::MICRO)
                        .color(p.error_text),
                ),
                _ => None,
            };

            let mut actions = row().key("q-actions").gap(space::XS).center_y();
            if matches!(
                entry.status,
                UploadStatus::Failed(_) | UploadStatus::Cancelled
            ) {
                actions = actions.child(btn_primary(
                    p,
                    &format!("q-retry-{clip_id}"),
                    "Retry",
                    move |st| st.retry_upload(clip_id),
                ));
            }
            if !matches!(entry.status, UploadStatus::Completed) {
                let cancel_label = if matches!(
                    entry.status,
                    UploadStatus::Failed(_) | UploadStatus::Cancelled
                ) {
                    "Dismiss"
                } else {
                    "Cancel"
                };
                actions = actions.child(btn_secondary(
                    p,
                    &format!("q-cancel-{clip_id}"),
                    cancel_label,
                    move |st| st.cancel_upload(clip_id),
                ));
            }

            rows = rows.child(
                col()
                    .key(format!("queue-item-{idx}-{clip_id}"))
                    .w_full()
                    .padding(space::SM)
                    .gap(space::XS)
                    .radius(radius::SM)
                    .fill(p.bg_lowest)
                    .border(1.0, p.border_subtle)
                    .child(
                        row()
                            .key("q-top")
                            .w_full()
                            .justify_between()
                            .center_y()
                            .gap(space::SM)
                            .child(
                                row()
                                    .key("q-title")
                                    .grow(1.0)
                                    .min_w(0.0)
                                    .gap(space::SM)
                                    .center_y()
                                    .child(
                                        text(entry.title.clone())
                                            .font(typography::LABEL)
                                            .color(p.text_primary),
                                    )
                                    .child(badge("q-status", &status_str, status_fg, status_bg)),
                            )
                            .child(actions),
                    )
                    .children(progress)
                    .children(err_note),
            );
        }
    }

    card(p, "transfer-queue-card")
        .w_full()
        .child(
            row()
                .key("queue-hdr")
                .w_full()
                .justify_between()
                .center_y()
                .child(
                    text("Transfer Queue")
                        .font(typography::HEADLINE_SM)
                        .color(p.text_primary),
                )
                .child(badge(
                    "queue-count",
                    &format!("{} items", s.uploads.len()),
                    p.text_secondary,
                    p.bg_elevated,
                )),
        )
        .child(rows)
}

fn inspector_sidebar(p: &Palette, s: &AppState, item: &SharedGridItem<'_>) -> El {
    let clip_id = item.clip_id;
    let local_clip = s.clips.iter().find(|c| c.id == clip_id);
    let share_id = item.share_id().map(str::to_string);
    let share_url = item.share_url().map(str::to_string);
    let is_public = item.is_public();

    let origin_text = match (item.upload.is_some(), item.remote.is_some()) {
        (true, true) => "Local + Remote",
        (true, false) => "Local Queue",
        (false, true) => "Remote Only",
        (false, false) => "Local",
    };

    let preview = if let Some(px) = s.library_thumbs.get(&clip_id) {
        el().key("insp-thumb")
            .w_full()
            .h(160.0)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .pixels_fit(px, SurfaceFit::Fit)
    } else {
        col()
            .key("insp-thumb-placeholder")
            .w_full()
            .h(160.0)
            .center()
            .gap(space::XS)
            .radius(radius::SM)
            .fill(p.bg_lowest)
            .border(1.0, p.border_subtle)
            .child(icon(&icons::CLOUD, 28.0).color(p.text_muted).a11y_hidden())
            .child(
                text(origin_text)
                    .font(typography::MONO_BADGE)
                    .color(p.text_muted),
            )
    };

    let game_val = item
        .game_name
        .clone()
        .or_else(|| local_clip.and_then(|c| c.game.as_ref().map(|g| g.name.clone())))
        .unwrap_or_else(|| "—".to_string());

    let size_val = item
        .remote
        .map(|r| format_bytes(r.size_bytes))
        .or_else(|| local_clip.map(|c| format_bytes(c.size_bytes)))
        .unwrap_or_else(|| "—".to_string());

    let duration_val = local_clip
        .map(|c| format_duration_ms(c.duration_ms))
        .unwrap_or_else(|| "—".to_string());

    let uploaded_val = item
        .remote
        .map(|r| r.uploaded_at.format("%Y-%m-%d %H:%M").to_string())
        .or_else(|| {
            item.upload.and_then(|u| {
                u.share
                    .as_ref()
                    .map(|sh| sh.uploaded_at.format("%Y-%m-%d %H:%M").to_string())
            })
        })
        .unwrap_or_else(|| "Pending".to_string());

    let local_mirror = local_clip
        .map(|c| c.file_path.display().to_string())
        .unwrap_or_else(|| "Remote only (not on this device)".to_string());

    let is_uploaded = item.remote.is_some()
        || item
            .upload
            .is_some_and(|u| matches!(u.status, UploadStatus::Completed));

    let link_section = if let Some(url) = share_url {
        let url_copy = url.clone();
        let url_open = url.clone();
        let sid_revoke = share_id.clone();
        Some(
            col()
                .key("insp-link-section")
                .w_full()
                .gap(space::XS)
                .child(
                    text("Share Link")
                        .font(typography::LABEL)
                        .color(p.text_secondary),
                )
                .child(
                    row()
                        .key("insp-url-box")
                        .w_full()
                        .padding(space::SM)
                        .radius(radius::XS)
                        .fill(p.bg_lowest)
                        .border(1.0, p.border_subtle)
                        .child(
                            text(url)
                                .font(typography::MONO_METRIC)
                                .color(p.text_primary)
                                .grow(1.0)
                                .min_w(0.0),
                        ),
                )
                .child(
                    row()
                        .key("insp-link-actions")
                        .w_full()
                        .gap(space::SM)
                        .child(
                            btn_primary(p, "insp-copy-link", "Copy Link", move |st| {
                                st.copy_to_clipboard(url_copy.clone());
                            })
                            .grow(1.0),
                        )
                        .child(
                            btn_secondary(p, "insp-open-link", "Open Page", move |st| {
                                st.open_url(url_open.clone());
                            })
                            .grow(1.0),
                        ),
                )
                .children(sid_revoke.map(|sid| {
                    btn_ghost(p, "insp-revoke-link", "Revoke Share Link", move |st| {
                        st.revoke_share_link(clip_id, sid.clone());
                    })
                    .w_full()
                })),
        )
    } else if is_uploaded {
        Some(
            col()
                .key("insp-link-create-section")
                .w_full()
                .gap(space::XS)
                .child(
                    text("Share Link")
                        .font(typography::LABEL)
                        .color(p.text_secondary),
                )
                .child(
                    btn_primary(p, "insp-create-link", "Create Share Link", move |st| {
                        st.create_share_link(clip_id, is_public);
                    })
                    .w_full(),
                ),
        )
    } else {
        None
    };

    let visibility_section = share_id.as_ref().map(|sid| {
        let sid_pub = sid.clone();
        let sid_priv = sid.clone();
        col()
            .key("insp-vis-section")
            .w_full()
            .gap(space::XS)
            .child(
                text("Visibility")
                    .font(typography::LABEL)
                    .color(p.text_secondary),
            )
            .child(
                row()
                    .key("insp-vis-pills")
                    .w_full()
                    .gap(space::SM)
                    .child(
                        pill_option(p, "insp-vis-public", "Public", is_public, move |st| {
                            set_visibility(st, clip_id, &sid_pub, true);
                        })
                        .grow(1.0),
                    )
                    .child(
                        pill_option(p, "insp-vis-private", "Private", !is_public, move |st| {
                            set_visibility(st, clip_id, &sid_priv, false);
                        })
                        .grow(1.0),
                    ),
            )
    });

    let delete_section = share_id.map(|sid| {
        let confirming = s.confirm_remote_delete.as_deref() == Some(sid.as_str());
        if confirming {
            col()
                .key("insp-delete-confirm")
                .w_full()
                .padding(space::SM)
                .gap(space::SM)
                .radius(radius::SM)
                .fill(p.recording_soft)
                .border(1.0, p.recording)
                .child(
                    text("Delete this clip from the remote server? Local file on disk remains untouched.")
                        .font(typography::LABEL)
                        .color(p.error_text),
                )
                .child(
                    row()
                        .key("insp-delete-confirm-actions")
                        .w_full()
                        .gap(space::SM)
                        .child(
                            btn_secondary(p, "insp-delete-cancel", "Cancel", |st| {
                                st.cancel_delete_remote_share();
                            })
                            .grow(1.0),
                        )
                        .child(
                            btn_danger(p, "insp-delete-ok", "Confirm Delete", |st| {
                                st.confirm_delete_remote_share();
                            })
                            .grow(1.0),
                        ),
                )
        } else {
            btn_danger(p, "insp-delete-btn", "Delete from Server", move |st| {
                st.request_delete_remote_share(sid.clone());
            })
            .w_full()
        }
    });

    panel(p, "upload-inspector")
        .w(320.0)
        .child(
            row()
                .key("insp-hdr")
                .w_full()
                .h(44.0)
                .px(space::LG)
                .center_y()
                .justify_between()
                .border_sides([0.0, 0.0, 1.0, 0.0], p.border_subtle)
                .child(
                    text("CLIP INSPECTOR")
                        .font(typography::MONO_BADGE)
                        .color(p.text_muted),
                )
                .child(icon_btn_sized(
                    p,
                    "insp-close",
                    &icons::CLOSE,
                    26.0,
                    14.0,
                    "Close inspector",
                    false,
                    |st| st.selected_upload_id = None,
                )),
        )
        .child(
            col()
                .key("insp-body")
                .w_full()
                .padding(space::LG)
                .gap(space::MD)
                .child(preview)
                .child(
                    col()
                        .key("insp-title-block")
                        .w_full()
                        .gap(space::XS)
                        .child(
                            text(item.title.clone())
                                .font(typography::HEADLINE_SM)
                                .color(p.text_primary),
                        )
                        .child(badge(
                            "insp-origin-badge",
                            origin_text,
                            p.text_secondary,
                            p.bg_elevated,
                        )),
                )
                .child(
                    col()
                        .key("insp-kv")
                        .w_full()
                        .gap(2.0)
                        .child(kv_row(p, "kv-game", "Game", game_val))
                        .child(kv_row(p, "kv-size", "Size", size_val))
                        .child(kv_row(p, "kv-dur", "Duration", duration_val))
                        .child(kv_row(p, "kv-uploaded", "Uploaded", uploaded_val)),
                )
                .child(
                    col()
                        .key("insp-local-mirror")
                        .w_full()
                        .gap(2.0)
                        .child(
                            text("Local Mirror")
                                .font(typography::LABEL)
                                .color(p.text_secondary),
                        )
                        .child(
                            text(local_mirror)
                                .font(typography::MONO_BADGE)
                                .color(p.text_muted),
                        ),
                )
                .children(link_section)
                .children(visibility_section)
                .children(delete_section),
        )
}

fn set_visibility(st: &mut AppState, clip_id: ClipId, share_id: &str, make_public: bool) {
    let target_vis = if make_public {
        ShareVisibility::Public
    } else {
        ShareVisibility::Private
    };

    if let Some(u) = st.uploads.iter().find(|u| u.clip_id == clip_id) {
        if u.is_public != make_public {
            st.toggle_share_visibility(clip_id);
        }
        if let Some(r) = st.remote_shares.iter_mut().find(|r| r.share_id == share_id) {
            r.visibility = target_vis;
        }
        return;
    }

    if let Some(r) = st.remote_shares.iter_mut().find(|r| r.share_id == share_id) {
        r.visibility = target_vis;
    }
    if let Some(worker) = &st.worker {
        worker.send(WorkerCommand::SetShareVisibility {
            share_id: share_id.to_string(),
            visibility: target_vis,
            server_url: st.config.server_url.clone(),
            api_token: st.config.api_token.clone(),
        });
    }
}
