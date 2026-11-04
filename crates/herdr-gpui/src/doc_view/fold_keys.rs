//! Cmd/Ctrl digit keys that fold a document to a level.

use crate::controls::CTRL_SHORTCUTS;
use gpui::KeyBinding;

#[derive(Clone, PartialEq, serde::Deserialize, gpui::Action)]
#[action(namespace = doc_view, no_json)]
pub(super) struct FoldToLevel {
    pub(super) level: usize,
}

/// These win over the app keymap's Tab N and Reset Font Size keys only while
/// a document editor has focus, because in GPUI a context-less binding ties
/// with the focused DocEditor context and the later binding wins, so bind_keys
/// must bind these after the keymap.
/// Cmd-Alt digits are an alias; AltGr typing on Windows still types because
/// GPUI skips bindings for a key that prefers character input while the editor
/// accepts text.
pub(crate) fn fold_key_bindings() -> impl IntoIterator<Item = KeyBinding> {
    let primary = if CTRL_SHORTCUTS { "ctrl" } else { "cmd" };
    (0..=9).flat_map(move |level| {
        ["", "alt-"].into_iter().map(move |modifier| {
            let key = format!("{primary}-{modifier}{level}");
            KeyBinding::new(&key, FoldToLevel { level }, Some("DocEditor"))
        })
    })
}
