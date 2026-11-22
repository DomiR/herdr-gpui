use super::super::{TERMINAL_CONTEXT, terminal_keys};
use moonframe::gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke, actions};

actions!(toolkit, [NextControl]);

/// What the toolkit's `Root` binds, as it does under every Moonframe window.
fn keymap() -> Keymap {
    let mut bindings = vec![
        KeyBinding::new("tab", NextControl, Some("Root")),
        KeyBinding::new("shift-tab", NextControl, Some("Root")),
    ];
    bindings.extend(terminal_keys());
    Keymap::new(bindings)
}

/// How many bindings `key` matches with `contexts` focused.
fn bound(keymap: &Keymap, key: &'static str, contexts: &[&str]) -> usize {
    let input: Vec<Keystroke> = Keystroke::parse(key).into_iter().collect();
    assert_eq!(input.len(), 1, "{key}");
    let stack: Vec<KeyContext> = contexts
        .iter()
        .filter_map(|context| KeyContext::parse(context).ok())
        .collect();
    assert_eq!(stack.len(), contexts.len());
    let (bindings, pending) = keymap.bindings_for_input(&input, &stack);
    assert!(!pending);
    bindings.len()
}

#[test]
fn tab_reaches_the_terminal_rather_than_moving_focus() {
    let keymap = keymap();
    for key in ["tab", "shift-tab"] {
        let over = ["Root", "MoonframeApp", TERMINAL_CONTEXT];
        assert_eq!(bound(&keymap, key, &over), 0, "{key}");
    }
}

#[test]
fn tab_still_moves_focus_outside_sheprds_regions() {
    assert_eq!(bound(&keymap(), "tab", &["Root", "MoonframeApp"]), 1);
}
