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
/// Each level's keys, plain and with Alt: GPUI takes a binding's keys only as
/// static text.
const LEVEL_KEYS: [[&str; 10]; 2] = if CTRL_SHORTCUTS {
    [
        [
            "ctrl-0", "ctrl-1", "ctrl-2", "ctrl-3", "ctrl-4", "ctrl-5", "ctrl-6", "ctrl-7",
            "ctrl-8", "ctrl-9",
        ],
        [
            "ctrl-alt-0",
            "ctrl-alt-1",
            "ctrl-alt-2",
            "ctrl-alt-3",
            "ctrl-alt-4",
            "ctrl-alt-5",
            "ctrl-alt-6",
            "ctrl-alt-7",
            "ctrl-alt-8",
            "ctrl-alt-9",
        ],
    ]
} else {
    [
        [
            "cmd-0", "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8",
            "cmd-9",
        ],
        [
            "cmd-alt-0",
            "cmd-alt-1",
            "cmd-alt-2",
            "cmd-alt-3",
            "cmd-alt-4",
            "cmd-alt-5",
            "cmd-alt-6",
            "cmd-alt-7",
            "cmd-alt-8",
            "cmd-alt-9",
        ],
    ]
};

pub(crate) fn fold_key_bindings() -> impl IntoIterator<Item = KeyBinding> {
    (0..=9).flat_map(|level| {
        LEVEL_KEYS
            .into_iter()
            .map(move |keys| KeyBinding::new(keys[level], FoldToLevel { level }, Some("DocEditor")))
    })
}
