//! The text area's keys: moving and selecting by character, word, row,
//! and line; deleting; the clipboard; and the shortcuts it takes from the
//! app while focused (`shortcut`): herdr-dog's Cmd-B, I, E, and K, Undo and
//! Redo, Cmd-Up and Down, and Cmd-Enter to send.

use super::{TextArea, TextAreaEvent, edit::Format};
use crate::{actions, words};
use gpui::{prelude::*, *};

impl TextArea {
    /// Handles `keystroke` before the app's key bindings see it, when it is
    /// one of the field's shortcuts and the field holds the keyboard.
    pub(super) fn shortcut(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) || self.is_composing() {
            return false;
        }
        let modifiers = keystroke.modifiers;
        let primary = actions::edit_modifiers(modifiers);
        let key = keystroke.key.as_str();
        // Cmd-Enter sends, as Ctrl-Enter does in herdr-dog.
        if key == "enter" && (modifiers.platform || modifiers.control) && !modifiers.alt {
            cx.emit(TextAreaEvent::Submit);
            return true;
        }
        if self.preview {
            return false;
        }
        let format = match key {
            "b" if primary => Some(Format::Bold),
            "i" if primary => Some(Format::Italic),
            "e" if primary => Some(Format::Code),
            "k" if primary => Some(Format::Link),
            _ => None,
        };
        if let Some(format) = format {
            self.format(format, cx);
            return true;
        }
        let shifted = modifiers.shift && (modifiers.platform || modifiers.control);
        match key {
            "z" if primary => {
                if self.edit.undo() {
                    self.edited(cx);
                }
            }
            "z" if shifted && !modifiers.alt => {
                if self.edit.redo() {
                    self.edited(cx);
                }
            }
            "up" | "down" if modifiers.platform && !modifiers.alt && !modifiers.control => {
                let offset = if key == "up" { 0 } else { self.edit.text.len() };
                self.edit.select_to(offset, modifiers.shift);
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Let the OS consume composition keys, picker navigation included.
        if self.is_composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let plain = !modifiers.platform && !modifiers.control && !modifiers.alt;
        let edit = actions::edit_modifiers(modifiers) && matches!(key, "a" | "c" | "v" | "x");
        if key == "escape" && plain && !modifiers.shift {
            cx.emit(TextAreaEvent::Cancel);
        } else if key == "enter" && plain {
            self.edit.newline();
            self.edited(cx);
        } else if edit {
            self.clipboard(key, cx);
        } else if modifiers.platform && !modifiers.control && !modifiers.alt {
            // As in every macOS text field: Cmd-Left and Right go to the
            // line's ends, Cmd-Backspace and Delete delete to them.
            let cursor = self.edit.cursor;
            match key {
                "left" => self
                    .edit
                    .select_to(self.edit.line_start(cursor), modifiers.shift),
                "right" => self
                    .edit
                    .select_to(self.edit.line_end(cursor), modifiers.shift),
                "backspace" | "delete" if !modifiers.shift => {
                    if self.edit.selection().is_empty() {
                        let to = if key == "backspace" {
                            self.edit.line_start(cursor)
                        } else {
                            self.edit.line_end(cursor)
                        };
                        self.edit.select_to(to, true);
                    }
                    self.delete(cx);
                }
                _ => return,
            }
        } else if plain {
            let selection = self.edit.selection();
            let cursor = self.edit.cursor;
            match key {
                "backspace" | "delete" => {
                    if selection.is_empty() {
                        let to = if key == "backspace" {
                            self.edit.previous(cursor)
                        } else {
                            self.edit.next(cursor)
                        };
                        self.edit.select_to(to, true);
                    }
                    self.delete(cx);
                }
                "left" if !modifiers.shift && !selection.is_empty() => {
                    self.edit.select_to(selection.start, false);
                }
                "right" if !modifiers.shift && !selection.is_empty() => {
                    self.edit.select_to(selection.end, false);
                }
                "left" => self
                    .edit
                    .select_to(self.edit.previous(cursor), modifiers.shift),
                "right" => self.edit.select_to(self.edit.next(cursor), modifiers.shift),
                "home" => self
                    .edit
                    .select_to(self.edit.line_start(cursor), modifiers.shift),
                "end" => self
                    .edit
                    .select_to(self.edit.line_end(cursor), modifiers.shift),
                "up" | "down" => {
                    let to = self.row_offset(key == "up");
                    self.edit.select_to(to, modifiers.shift);
                }
                _ => return,
            }
        } else if actions::word_modifiers(modifiers)
            && matches!(key, "backspace" | "delete" | "left" | "right")
        {
            // macOS sends these as commands, never text.
            let selection = self.edit.selection();
            let from = match key {
                "left" if !modifiers.shift && !selection.is_empty() => selection.start,
                "right" if !modifiers.shift && !selection.is_empty() => selection.end,
                _ => self.edit.cursor,
            };
            let to = if matches!(key, "backspace" | "left") {
                words::previous_start(&self.edit.text, from)
            } else {
                words::next_end(&self.edit.text, from)
            };
            if matches!(key, "left" | "right") {
                self.edit.select_to(to, modifiers.shift);
            } else {
                if selection.is_empty() {
                    self.edit.select_to(to, true);
                }
                self.delete(cx);
            }
        } else {
            // Option and dead-key text goes through the platform handler.
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        if self.edit.replace(None, "", false, None) {
            self.edited(cx);
        }
    }

    fn clipboard(&mut self, key: &str, cx: &mut Context<Self>) {
        let selection = self.edit.selection();
        match key {
            "a" => self.edit.select_all(),
            "c" | "x" if !selection.is_empty() => {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    self.edit.text[selection].to_owned(),
                ));
                if key == "x" {
                    self.delete(cx);
                }
            }
            "v" => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
                    && self.edit.replace(None, &text, false, None)
                {
                    self.edited(cx);
                }
            }
            _ => {}
        }
    }

    /// The offset a row up or down from the caret, at its column; the ends
    /// of the text past the first and last rows.
    fn row_offset(&self, up: bool) -> usize {
        let cursor = self.edit.cursor;
        let Some(layout) = &self.layout else {
            return if up { 0 } else { self.edit.text.len() };
        };
        let Some(at) = layout.position(cursor) else {
            return cursor;
        };
        let line_height = layout.line_height;
        if up && at.y < line_height {
            return 0;
        }
        let y = if up {
            at.y - line_height
        } else {
            at.y + line_height
        };
        if !up && y >= line_height * layout.rows() as f32 {
            return self.edit.text.len();
        }
        layout
            .index_at(point(at.x, y + line_height / 2.))
            .min(self.edit.text.len())
    }
}
