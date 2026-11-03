//! Finding text in a document. Search stays on the UI thread because the
//! document is bounded and preparation already owns the text snapshot.
// i18n: migrated

use super::editor::{DocEditor, Selection};
use crate::{config::corners, i18n::fl, motion, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_doc::{Anchor, markdown, markdown::Line, todo};
use std::ops::Range;

/// The most matches retained for painting and stepping.
pub(super) const MAX_MATCHES: usize = 10_000;
/// Search input is single line text, so bounding it also bounds comparisons.
pub(super) const MAX_QUERY: usize = 1_024;

pub(super) struct Find {
    pub(super) shown: motion::Toggle,
    pub(super) input: Entity<SearchInput>,
    query: String,
    match_case: bool,
    pub(super) matches: Vec<Range<usize>>,
    pub(super) current: Option<usize>,
    pub(super) current_anchor: Option<Anchor>,
    origin: usize,
    capped: bool,
}

impl Find {
    pub(super) fn new(input: Entity<SearchInput>) -> Self {
        Self {
            shown: motion::Toggle::default(),
            input,
            query: String::new(),
            match_case: false,
            matches: Vec::new(),
            current: None,
            current_anchor: None,
            origin: 0,
            capped: false,
        }
    }

    fn clear_results(&mut self) {
        self.matches.clear();
        self.current = None;
        self.current_anchor = None;
        self.capped = false;
    }

    pub(super) fn recompute(
        &mut self,
        text: &str,
        lines: &[Line],
        hide_done: bool,
        current_offset: Option<usize>,
    ) {
        if !self.shown.is_open() {
            self.clear_results();
            return;
        }
        let (mut matches, capped) = find_all(text, &self.query, self.match_case, MAX_MATCHES);
        if hide_done {
            let done = todo::done_lines(lines);
            matches.retain(|range| {
                let line = markdown::line_at(lines, range.start);
                !done.get(line).copied().unwrap_or(false)
            });
        }
        self.matches = matches;
        self.capped = capped;
        let at = current_offset.unwrap_or(self.origin);
        self.current = if self.matches.is_empty() {
            None
        } else {
            self.matches
                .iter()
                .position(|range| range.start >= at)
                .or(Some(0))
        };
    }

    fn status(&self) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        if self.matches.is_empty() {
            return fl!("doc-view-find-none");
        }
        if self.capped {
            return fl!("doc-view-find-capped", total = self.matches.len());
        }
        fl!(
            "doc-view-find-match",
            current = self.current.map_or(0, |current| current + 1),
            total = self.matches.len()
        )
    }
}

fn capped_query(query: &str) -> &str {
    let end = query
        .char_indices()
        .map(|(at, _)| at)
        .chain([query.len()])
        .take_while(|at| *at <= MAX_QUERY)
        .last()
        .unwrap_or(0);
    &query[..end]
}

/// Finds non-overlapping byte ranges from left to right.
pub(super) fn find_all(
    text: &str,
    query: &str,
    match_case: bool,
    cap: usize,
) -> (Vec<Range<usize>>, bool) {
    let query = capped_query(query);
    if query.is_empty() {
        return (Vec::new(), false);
    }
    let needle = query.as_bytes();
    let mut matches = Vec::new();
    let mut start = 0;
    while start + needle.len() <= text.len() {
        let end = start + needle.len();
        let found = text.is_char_boundary(start)
            && text.is_char_boundary(end)
            && if match_case {
                text.as_bytes()[start..end].eq(needle)
            } else {
                text.as_bytes()[start..end].eq_ignore_ascii_case(needle)
            };
        if found {
            if matches.len() >= cap {
                return (matches, true);
            }
            matches.push(start..end);
            start = end;
        } else {
            start += 1;
        }
    }
    (matches, false)
}

impl DocEditor {
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.shown.is_open() {
            let input = self.find.input.clone();
            input.update(cx, |input, cx| {
                let text = input.text().to_owned();
                input.set_text_selected(&text, cx);
            });
            let focus = self.find.input.read(cx).focus.clone();
            window.focus(&focus, cx);
            cx.notify();
            return;
        }

        self.prepare(cx);
        let text = self.text(cx).to_owned();
        let selection = self.selection.range();
        let selected = (!selection.is_empty() && selection.len() <= MAX_QUERY)
            .then(|| text.get(selection.clone()))
            .flatten()
            .filter(|selected| !selected.contains('\r') && !selected.contains('\n'))
            .map(str::to_owned);
        self.find.origin = selection.start;
        if let Some(selected) = selected {
            self.find.query = selected;
        }
        self.find.current = None;
        self.find.current_anchor = None;
        self.find.shown.set(true, motion::now(), false);
        let query = self.find.query.clone();
        let input = self.find.input.clone();
        input.update(cx, |input, cx| input.set_text_selected(&query, cx));
        let focus = self.find.input.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.stale = true;
        self.prepare(cx);
        self.select_current_find(cx);
        cx.notify();
    }

    pub(crate) fn find_again(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.shown.is_open() {
            self.open_find(window, cx);
            return;
        }
        self.step_find(back, window, cx);
    }

    fn step_find(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.find.input.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.prepare(cx);
        let count = self.find.matches.len();
        let Some(next) = (count > 0).then(|| match (self.find.current, back) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(current), false) => (current + 1) % count,
            (Some(current), true) => (current + count - 1) % count,
        }) else {
            return;
        };
        self.find.current = Some(next);
        self.select_current_find(cx);
        cx.notify();
    }

    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.shown.is_open() && !self.find.shown.moving(motion::now()) {
            return;
        }
        self.find.shown.set(false, motion::now(), false);
        self.find.clear_results();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn find_changed(&mut self, input: Entity<SearchInput>, cx: &mut Context<Self>) {
        self.find.query = capped_query(input.read(cx).text()).to_owned();
        if !self.find.shown.is_open() {
            return;
        }
        self.find.current = None;
        self.find.current_anchor = None;
        self.stale = true;
        self.prepare(cx);
        self.select_current_find(cx);
        cx.notify();
    }

    pub(super) fn toggle_find_match_case(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find.shown.is_open() {
            return;
        }
        let focus = self.find.input.read(cx).focus.clone();
        window.focus(&focus, cx);
        self.find.match_case = !self.find.match_case;
        self.find.current = None;
        self.find.current_anchor = None;
        self.stale = true;
        self.prepare(cx);
        self.select_current_find(cx);
        cx.notify();
    }

    pub(super) fn select_current_find(&mut self, cx: &mut Context<Self>) {
        let Some(range) = self
            .find
            .current
            .and_then(|current| self.find.matches.get(current))
            .cloned()
        else {
            return;
        };
        self.find.current_anchor = self
            .buffer
            .read(cx)
            .document()
            .and_then(|doc| doc.anchor(range.start, false));
        let line = markdown::line_at(&self.prepared.lines, range.start);
        self.unfold_around(line, cx);
        let Some(range) = self
            .find
            .current
            .and_then(|current| self.find.matches.get(current))
            .cloned()
        else {
            return;
        };
        self.select(
            Selection {
                anchor: range.start,
                head: range.end,
            },
            cx,
        );
        self.reveal(cx);
        self.find.current_anchor = self
            .buffer
            .read(cx)
            .document()
            .and_then(|doc| doc.anchor(range.start, false));
    }

    fn find_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.input.read(cx).is_composing() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" if !modifiers.modified() => self.close_find(window, cx),
            "enter"
                if !modifiers.control
                    && !modifiers.alt
                    && !modifiers.platform
                    && !modifiers.function =>
            {
                self.step_find(modifiers.shift, window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    #[cfg(test)]
    pub(super) fn find_status(&self) -> String {
        self.find.status()
    }

    pub(super) fn render_find_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = motion::now();
        if !self.find.shown.is_open() && !self.find.shown.moving(now) {
            return None;
        }
        let shown = self.find.shown.shown(now);
        let theme = self.look.theme.clone();
        let status = self.find.status();
        let has_matches = !self.find.matches.is_empty();
        let match_case = self.find.match_case;
        let input = self.find.input.clone();
        let button = |id: &'static str, label: String, icon: Option<&'static str>| {
            let enabled = icon.is_none() || id == "doc-find-close" || has_matches;
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(Role::Button)
                .aria_label(SharedString::from(label))
                .flex_none()
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(corners::CONTROL))
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                })
                .when(match_case && icon.is_none(), |button| {
                    button.bg(rgb(theme.active))
                })
                .child(match icon {
                    Some(icon) => svg()
                        .path(icon)
                        .size(px(14.))
                        .text_color(rgb(if has_matches || id == "doc-find-close" {
                            theme.foreground
                        } else {
                            theme.muted
                        }))
                        .into_any_element(),
                    None => div()
                        .text_size(px(11.))
                        .text_color(rgb(theme.foreground))
                        .child("Aa") // i18n: data
                        .into_any_element(),
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        Some(
            div()
                .id("doc-find-bar")
                .debug_selector(|| "doc-find-bar".into())
                .absolute()
                .top(px(8.))
                .right(px(8.))
                .min_w_0()
                .w(px(320.))
                .flex_shrink(1.)
                .flex()
                .items_center()
                .gap(px(4.))
                .p(px(4.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.surface))
                .text_color(rgb(theme.foreground))
                .text_size(px(self.look.ui.size))
                .shadow_md()
                .occlude()
                .opacity(shown)
                .mt(px(-(1. - shown) * 4.))
                .on_key_down(cx.listener(Self::find_key_down))
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("doc-find-input")
                        .debug_selector(|| "doc-find-input".into())
                        .flex_1()
                        .min_w_0()
                        .child(input),
                )
                .child(
                    button("doc-find-match-case", fl!("doc-view-find-match-case"), None).on_click(
                        cx.listener(|this, _, window, cx| {
                            this.toggle_find_match_case(window, cx);
                        }),
                    ),
                )
                .child(
                    button(
                        "doc-find-previous",
                        fl!("doc-view-find-previous"),
                        Some("icons/chevron-up.svg"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.step_find(true, window, cx);
                    })),
                )
                .child(
                    button(
                        "doc-find-next",
                        fl!("doc-view-find-next"),
                        Some("icons/chevron-down.svg"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.step_find(false, window, cx);
                    })),
                )
                .child(
                    div()
                        .id("doc-find-count")
                        .debug_selector(|| "doc-find-count".into())
                        .flex_none()
                        .px(px(4.))
                        .text_color(rgb(theme.muted))
                        .child(status),
                )
                .child(
                    button(
                        "doc-find-close",
                        fl!("doc-view-find-close"),
                        Some("icons/close.svg"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_find(window, cx);
                    })),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests;
