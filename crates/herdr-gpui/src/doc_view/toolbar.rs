//! The document's title, write state, and wrapping editor controls.
// i18n: migrated

use super::{DocEditor, Request, State, render::PAD};
use crate::{
    direction::Logical,
    i18n::{Localize, fl},
};
use gpui::{prelude::*, *};

impl DocEditor {
    pub(super) fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.look.theme;
        let buffer = self.buffer.read(cx);
        let name = buffer
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let version = buffer.document().map(|doc| doc.version());
        let dirty = buffer.document().is_some_and(herdr_doc::Document::dirty);
        let banner = match buffer.state() {
            State::Loading => Some(fl!("doc-view-loading")),
            State::Failed(error) => Some(fl!("doc-view-failed", error = error.localize())),
            State::Ready if buffer.missing() => Some(fl!("doc-view-missing")),
            State::Ready => buffer
                .write_error()
                .map(|error| fl!("doc-view-write-failed", error = error.localize())),
        };
        let link = crate::doc_link::presentation(
            buffer.path(),
            self.link_supported,
            version.is_some(),
            cx,
        );
        let markdown = super::is_markdown(buffer.path());
        let button = |id: &'static str, label: String| {
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(label.clone())
                .px(px(8.))
                .py(px(2.))
                .rounded(px(4.))
                .cursor_pointer()
                .text_color(rgb(theme.muted))
                .hover(|style| style.bg(rgb(theme.surface)))
                .child(label)
        };
        let hide = if self.hide_done {
            fl!("doc-view-show-done")
        } else {
            fl!("doc-view-hide-done")
        };
        let foreground = theme.foreground;
        let surface = theme.surface;
        let tooltip: SharedString = link.tooltip.into();
        let controls = div()
            .flex()
            .row()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .when(markdown, |bar| {
                bar.child(
                    div()
                        .id("doc-agent-link")
                        .role(Role::Button)
                        .aria_label(link.label.clone())
                        .px(px(8.))
                        .py(px(2.))
                        .rounded(px(4.))
                        .text_color(rgb(if link.on {
                            theme.foreground
                        } else {
                            theme.muted
                        }))
                        .when(link.on, |button| button.bg(rgb(theme.surface)))
                        .when(!link.enabled, |button| button.opacity(0.55))
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::usage::Hint {
                                text: tooltip.clone(),
                                foreground,
                                surface,
                            })
                            .into()
                        })
                        .when(link.enabled, |button| {
                            button
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme.surface)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.request(Request::ToggleAgentLink, cx);
                                }))
                        })
                        .child(link.label),
                )
            })
            .when(self.comment_count() > 0, |bar| {
                bar.child(
                    button(
                        "doc-send-comments",
                        fl!("doc-view-comments-send", count = self.comment_count()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.send_comments(None, cx))),
                )
            })
            .child(
                button("doc-hide-done", hide)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_hide_done(cx))),
            )
            .child(
                button("doc-fold-all", fl!("doc-view-fold-all"))
                    .on_click(cx.listener(|this, _, _, cx| this.fold_all(true, cx))),
            )
            .child(
                button("doc-unfold-all", fl!("doc-view-unfold-all"))
                    .on_click(cx.listener(|this, _, _, cx| this.fold_all(false, cx))),
            );
        div()
            .flex_none()
            .flex()
            .flex_col()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(4.))
                    .px(px(PAD))
                    .py(px(6.))
                    .border_b_1()
                    .border_color(rgb(theme.surface))
                    .text_size(px(self.look.ui.size * 0.9))
                    .child(
                        div()
                            .flex()
                            .row()
                            .items_center()
                            .gap(px(4.))
                            .flex_1()
                            .min_w(px(80.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .text_color(rgb(theme.foreground))
                                    .child(name),
                            )
                            .when_some(version, |bar, version| {
                                bar.child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(theme.muted))
                                        .child(fl!("doc-view-version", version = version.0)),
                                )
                            })
                            .when(dirty, |bar| {
                                bar.child(
                                    div()
                                        .flex_none()
                                        .size(px(6.))
                                        .rounded_full()
                                        .bg(rgb(theme.muted)),
                                )
                            }),
                    )
                    .child(controls),
            )
            .when_some(banner, |header, banner| {
                header.child(
                    div()
                        .px(px(PAD))
                        .py(px(6.))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.foreground))
                        .child(banner),
                )
            })
            .into_any_element()
    }
}
