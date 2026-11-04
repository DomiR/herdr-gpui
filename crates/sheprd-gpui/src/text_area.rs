//! A multi-line, Markdown-aware text field, like herdr-dog's comment
//! composer: soft-wrapped text that grows with what is typed up to a
//! height, then scrolls; a toolbar of Markdown formats with Cmd-B, I, E, and
//! K; Write and Preview; Cmd-Enter to send and Escape to cancel, as events
//! its owner acts on. Input methods compose through the platform handler,
//! as in `SearchInput`.
//!
//! The app's own shortcuts are key bindings, which GPUI resolves before a
//! focused element sees the key, so the field intercepts the few it uses
//! (`keys::SHORTCUTS`) while it holds the keyboard.

mod edit;
mod keys;
mod paint;

#[cfg(test)]
mod tests;

use crate::{
    actions,
    config::{FontConfig, Theme, corners},
    fonts::StyledFont,
};
use edit::Editing;
pub(crate) use edit::Format;
use gpui::{prelude::*, *};
use std::ops::Range;

/// What the field tells its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAreaEvent {
    Changed,
    /// Cmd-Enter, or Ctrl-Enter.
    Submit,
    /// Escape.
    Cancel,
}

/// The text as last drawn: each logical line wrapped, and where it starts.
struct Layout {
    lines: Vec<WrappedLine>,
    starts: Vec<usize>,
    line_height: Pixels,
}

impl Layout {
    /// Where `offset` draws, from the text's top left.
    fn position(&self, offset: usize) -> Option<Point<Pixels>> {
        let mut top = px(0.);
        for (line, start) in self.lines.iter().zip(&self.starts) {
            if offset <= start + line.len() {
                let at =
                    line.position_for_index(offset.saturating_sub(*start), self.line_height)?;
                return Some(point(at.x, top + at.y));
            }
            top += line.size(self.line_height).height;
        }
        None
    }

    /// The offset nearest `position`, from the text's top left.
    fn index_at(&self, position: Point<Pixels>) -> usize {
        let mut top = px(0.);
        let last = self.lines.len().saturating_sub(1);
        for (index, (line, start)) in self.lines.iter().zip(&self.starts).enumerate() {
            let height = line.size(self.line_height).height;
            if position.y < top + height || index == last {
                let local = point(position.x, (position.y - top).max(px(0.)));
                let found = match line.closest_index_for_position(local, self.line_height) {
                    Ok(found) | Err(found) => found,
                };
                return start + found.min(line.len());
            }
            top += height;
        }
        0
    }

    /// Visual rows the text takes.
    fn rows(&self) -> usize {
        self.lines
            .iter()
            .map(|line| (line.size(self.line_height).height / self.line_height).round() as usize)
            .sum::<usize>()
            .max(1)
    }
}

pub(crate) struct TextArea {
    pub(crate) focus: FocusHandle,
    edit: Editing,
    placeholder: SharedString,
    font: FontConfig,
    theme: Theme,
    layout: Option<Layout>,
    /// Where the text drew last.
    bounds: Option<Bounds<Pixels>>,
    /// How far the text is scrolled up, once it is taller than the field.
    scroll: Pixels,
    selecting: bool,
    min_rows: usize,
    max_rows: usize,
    /// Visual rows the text took when last drawn, which sizes the field.
    rows: usize,
    /// Showing the Markdown drawn rather than the text.
    preview: bool,
    _intercept: Subscription,
}

impl EventEmitter<TextAreaEvent> for TextArea {}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TextArea {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let config = crate::config::Config::default();
        let this = cx.weak_entity();
        // The field's shortcuts win over the app's while it holds the
        // keyboard; see the module comment.
        let intercept = cx.intercept_keystrokes(move |event, window, cx| {
            let Some(area) = this.upgrade() else {
                return;
            };
            let handled = area.update(cx, |area, cx| area.shortcut(&event.keystroke, window, cx));
            if handled {
                cx.stop_propagation();
            }
        });
        Self {
            focus: cx.focus_handle(),
            edit: Editing::default(),
            placeholder: SharedString::default(),
            font: config.ui.clone(),
            theme: Theme::default(),
            layout: None,
            bounds: None,
            scroll: px(0.),
            selecting: false,
            min_rows: 3,
            max_rows: 14,
            rows: 1,
            preview: false,
            _intercept: intercept,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.edit.text
    }

    pub(crate) fn set_placeholder(&mut self, placeholder: &str, cx: &mut Context<Self>) {
        self.placeholder = placeholder.to_owned().into();
        cx.notify();
    }

    pub(crate) fn set_appearance(
        &mut self,
        font: FontConfig,
        theme: Theme,
        cx: &mut Context<Self>,
    ) {
        self.font = font;
        self.theme = theme;
        self.layout = None;
        cx.notify();
    }

    /// What decides the field's height: the rows it shows, and whether
    /// it previews; an owner that caches heights measures again on change.
    pub(crate) fn shape(&self) -> (usize, bool) {
        (self.rows.clamp(self.min_rows, self.max_rows), self.preview)
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.edit.marked.is_some()
    }

    /// Applies `format` at the selection, as its toolbar button does.
    pub(crate) fn format(&mut self, format: Format, cx: &mut Context<Self>) {
        self.preview = false;
        self.edit.format(format);
        self.edited(cx);
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        cx.emit(TextAreaEvent::Changed);
        cx.notify();
    }

    fn line_height(&self) -> Pixels {
        px(self.font.line_height())
    }

    /// The offset under `position`, in window coordinates.
    fn index_at(&self, position: Point<Pixels>) -> usize {
        match (&self.layout, self.bounds) {
            (Some(layout), Some(bounds)) => layout.index_at(point(
                position.x - bounds.left(),
                position.y - bounds.top() + self.scroll,
            )),
            _ => self.edit.text.len(),
        }
    }
}

fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].encode_utf16().count()
}

fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        if units >= offset {
            return at;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = from_utf16(text, range.start);
    start..from_utf16(text, range.end).max(start)
}

impl EntityInputHandler for TextArea {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.edit.text;
        let range = byte_range(text, range);
        *actual = Some(to_utf16(text, range.start)..to_utf16(text, range.end));
        Some(text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = &self.edit.text;
        let selection = self.edit.selection();
        Some(UTF16Selection {
            range: to_utf16(text, selection.start)..to_utf16(text, selection.end),
            reversed: self.edit.cursor < self.edit.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let text = &self.edit.text;
        self.edit
            .marked
            .as_ref()
            .map(|marked| to_utf16(text, marked.start)..to_utf16(text, marked.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.edit.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range.map(|range| byte_range(&self.edit.text, range));
        if self.edit.replace(range, text, false, None) {
            self.edited(cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range.map(|range| byte_range(&self.edit.text, range));
        let selected = selected.map(|selected| byte_range(text, selected));
        if self.edit.replace(range, text, true, selected) {
            self.edited(cx);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        let bounds = self.bounds?;
        let range = byte_range(&self.edit.text, range);
        let at = layout.position(range.start)?;
        Some(Bounds::new(
            point(bounds.left() + at.x, bounds.top() + at.y - self.scroll),
            size(px(1.), layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.layout.as_ref()?;
        Some(to_utf16(&self.edit.text, self.index_at(position)))
    }
}

/// The toolbar's buttons, with the key each takes under Cmd.
const BUTTONS: &[(Format, &str, &str, Option<&str>)] = &[
    (Format::Heading, "text-area-heading", "H", None),
    (Format::Bold, "text-area-bold", "B", Some("B")),
    (Format::Italic, "text-area-italic", "I", Some("I")),
    (Format::Code, "text-area-code", "</>", Some("E")),
    (Format::Link, "text-area-link", "Link", Some("K")),
    (Format::Quote, "text-area-quote", "\u{275d}", None),
    (Format::Bullets, "text-area-bullets", "\u{2022}", None),
    (Format::Tasks, "text-area-tasks", "\u{2610}", None),
];

impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.clone();
        let (foreground, surface) = (theme.foreground, theme.surface);
        let focused = self.focus.is_focused(window);
        let small = px(self.font.size - 1.);
        let tab = |id: &'static str, label: &'static str, on: bool, preview: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px_2()
                .py(px(2.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .text_size(small)
                .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                .when(on, |tab| tab.bg(rgb(theme.active)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.preview = preview;
                    if !preview {
                        window.focus(&this.focus, cx);
                    }
                    cx.notify();
                }))
                .child(label)
        };
        let tools = BUTTONS.iter().map(|(format, id, label, key)| {
            let format = *format;
            let id = *id;
            let tip: SharedString = match key {
                Some(key) => format!("{format:?} (\u{2318}{key})").into(),
                None => format!("{format:?}").into(),
            };
            div()
                .id(id)
                .debug_selector(move || id.into())
                .min_w(px(22.))
                .h(px(20.))
                .px_1()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .text_size(small)
                .text_color(rgb(theme.subtext()))
                .hover(|style| style.bg(rgb(theme.active)))
                .tooltip(move |_, cx| {
                    cx.new(|_| crate::menu::SettingsHint {
                        text: tip.clone(),
                        foreground,
                        surface,
                    })
                    .into()
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.format(format, cx);
                    window.focus(&this.focus, cx);
                }))
                .child(*label)
        });
        let toolbar = div()
            .flex()
            .items_center()
            .gap_1()
            .pb_1()
            .child(tab("text-area-write", "Write", !self.preview, false))
            .child(tab("text-area-preview", "Preview", self.preview, true))
            .child(div().flex_1())
            .when(!self.preview, |bar| bar.children(tools));
        let body = if self.preview {
            let blocks = crate::markdown_view::parse(&self.edit.text);
            let look = crate::orchestrator::Look {
                theme: self.theme.clone(),
                ui: self.font.clone(),
                mono: self.font.clone(),
            };
            div()
                .min_h(self.line_height() * self.min_rows as f32)
                .child(if blocks.is_empty() {
                    div()
                        .text_color(rgb(self.theme.muted))
                        .child("Nothing to preview")
                        .into_any_element()
                } else {
                    crate::markdown_view::render("text-area-preview-body", &blocks, &look, cx)
                        .into_any_element()
                })
                .into_any_element()
        } else {
            self.render_editor(cx).into_any_element()
        };
        div()
            .debug_selector(|| "text-area".into())
            .w_full()
            .flex()
            .flex_col()
            .p_2()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(if focused {
                self.theme.primary()
            } else {
                self.theme.active
            }))
            .bg(rgb(self.theme.background))
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.font)
            .text_size(px(self.font.size))
            .child(toolbar)
            .child(body)
    }
}

impl TextArea {
    fn render_editor(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let rows = self.rows.clamp(self.min_rows, self.max_rows);
        let height = self.line_height() * rows as f32;
        div()
            .id("text-area-editor")
            .role(Role::MultilineTextInput)
            .aria_label(self.placeholder.clone())
            .w_full()
            .h(height)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .line_height(self.line_height())
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &actions::Cut, window, cx| {
                this.key_down(&actions::edit_key("x"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::Copy, window, cx| {
                this.key_down(&actions::edit_key("c"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::Paste, window, cx| {
                this.key_down(&actions::edit_key("v"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::SelectAll, window, cx| {
                this.key_down(&actions::edit_key("a"), window, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                    if this.is_composing() {
                        return;
                    }
                    let offset = this.index_at(event.position);
                    this.edit.select_to(offset, event.modifiers.shift);
                    if event.click_count == 2 {
                        let start = crate::words::previous_start(&this.edit.text, offset);
                        let end = crate::words::next_end(&this.edit.text, start);
                        this.edit.anchor = start;
                        this.edit.cursor = end;
                    } else if event.click_count >= 3 {
                        this.edit.anchor = this.edit.line_start(offset);
                        this.edit.cursor = this.edit.line_end(offset);
                    }
                    this.selecting = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.selecting && !this.is_composing() {
                    this.edit.select_to(this.index_at(event.position), true);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let delta = event.delta.pixel_delta(this.line_height()).y;
                this.scroll_by(-delta);
                let _ = window;
                cx.notify();
            }))
            .child(self.canvas(cx))
    }

    fn scroll_by(&mut self, delta: Pixels) {
        let line_height = self.line_height();
        let content = line_height * self.rows as f32;
        let shown = line_height * self.rows.clamp(self.min_rows, self.max_rows) as f32;
        self.scroll = (self.scroll + delta).clamp(px(0.), (content - shown).max(px(0.)));
    }
}
