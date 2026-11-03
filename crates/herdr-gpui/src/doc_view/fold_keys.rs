//! Cmd/Ctrl digit keys that fold a document to a level.

use crate::controls::CTRL_SHORTCUTS;
use gpui::KeyBinding;

#[derive(Clone, PartialEq, serde::Deserialize, gpui::Action)]
#[action(namespace = doc_view, no_json)]
pub(crate) struct FoldToLevel {
    pub(crate) level: usize,
}

/// These win over the app keymap's Tab N and Reset Font Size keys only while
/// a document editor has focus, because in GPUI a context-less binding ties
/// with the focused DocEditor context and the later binding wins, so bind_keys
/// must bind these after the keymap.
pub(crate) fn fold_key_bindings() -> impl IntoIterator<Item = KeyBinding> {
    let primary = if CTRL_SHORTCUTS { "ctrl" } else { "cmd" };
    (0..=9).map(move |level| {
        KeyBinding::new(
            &format!("{primary}-{level}"),
            FoldToLevel { level },
            Some("DocEditor"),
        )
    })
}
