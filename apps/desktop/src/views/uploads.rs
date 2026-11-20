//! Uploads view: upload queue/history, shareable link copy, and public/private toggle.

use rune::tok::type_;
use rune::{El, col, row, text};

use crate::components::{badge, btn_secondary, card, pill_option, section_header};
use crate::state::{AppState, UploadStatus};
use crate::theme::{Palette, radius, space};

pub fn view(p: &Palette, s: &AppState) -> El {
    let server_label = s
        .config
        .server_url
        .as_deref()
        .unwrap_or("Not configured (set in Settings)");

    let header_card = card(p, "uploads-server-card").child(
        row()
            .key("uploads-hdr-row")
            .w_full()
            .justify_between()
            .center_y()
            .child(section_header(
                p,
                "uploads-hdr",
                "Cloud Shares & Uploads",
                "Manage uploaded clips and shareable links",
            ))
            .child(badge(
                "uploads-server-badge",
                server_label,
                p.text_secondary,
                p.bg_elevated,
            )),
    );

    let mut list = col().key("uploads-list").w_full().gap(space::SM);
    if s.uploads.is_empty() {
        list = list.child(
            card(p, "uploads-empty")
                .center()
                .py(space::XXL)
                .child(
                    text("No uploads yet")
                        .font(type_::SECTION)
                        .color(p.text_primary),
                )
                .child(
                    text("Use Share on any clip in the library to queue an upload")
                        .font(type_::LABEL)
                        .color(p.text_secondary),
                ),
        );
    } else {
        for (idx, item) in s.uploads.iter().enumerate() {
            let clip_id = item.clip_id;
            let (status_str, status_fg, status_bg) = match &item.status {
                UploadStatus::Queued => ("Queued".to_string(), p.warning, p.warning_soft),
                UploadStatus::Uploading { progress_pct } => (
                    format!("Uploading {progress_pct}%"),
                    p.accent_text,
                    p.accent_soft,
                ),
                UploadStatus::Completed => ("Live".to_string(), p.success, p.success_soft),
                UploadStatus::Failed(_) => ("Failed".to_string(), p.recording, p.recording_soft),
            };
            let share_url = item
                .share
                .as_ref()
                .map(|sh| sh.url.clone())
                .unwrap_or_else(|| "Pending upload".to_string());

            list = list.child(
                row()
                    .key(format!("upload-{idx}-{clip_id}"))
                    .w_full()
                    .padding(space::MD)
                    .justify_between()
                    .center_y()
                    .fill(p.bg_surface)
                    .border(1.0, p.border_subtle)
                    .radius(radius::MD)
                    .child(
                        col()
                            .key("meta")
                            .gap(4.0)
                            .child(
                                row()
                                    .key("title-row")
                                    .gap(space::SM)
                                    .center_y()
                                    .child(
                                        text(item.title.clone())
                                            .font(type_::BODY)
                                            .color(p.text_primary),
                                    )
                                    .child(badge("status", &status_str, status_fg, status_bg)),
                            )
                            .child(
                                text(format!("{} • {share_url}", item.game_name))
                                    .font(type_::LABEL)
                                    .color(p.text_secondary),
                            ),
                    )
                    .child(
                        row()
                            .key("actions")
                            .gap(space::SM)
                            .child(pill_option(
                                p,
                                &format!("vis-{clip_id}"),
                                if item.is_public { "Public" } else { "Private" },
                                item.is_public,
                                move |st| {
                                    st.toggle_share_visibility(clip_id);
                                },
                            ))
                            .child(btn_secondary(
                                p,
                                &format!("copy-{clip_id}"),
                                "Copy Link",
                                move |st| {
                                    if let Some(u) =
                                        st.uploads.iter().find(|u| u.clip_id == clip_id)
                                    {
                                        let url = u
                                            .share
                                            .as_ref()
                                            .map(|sh| sh.url.as_str())
                                            .unwrap_or("pending");
                                        st.status_banner =
                                            Some(format!("Copied share link: {url}"));
                                    }
                                },
                            )),
                    ),
            );
        }
    }

    col()
        .key("uploads-view")
        .w_full()
        .gap(space::LG)
        .child(header_card)
        .child(list)
}
