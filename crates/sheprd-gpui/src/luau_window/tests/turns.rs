//! A chat's turns: what the `turns` mod's timeline sends Sheprd.
use super::super::{Action, Data, action};

fn tick(revision: &str) -> Option<Action> {
    // The timeline adds `revision`, `index`, and `label` to `select`.
    action(&Data::map([
        ("chat_turn", "w1:p1".into()),
        ("revision", revision.into()),
        ("index", 2.0.into()),
        ("label", "Fix it".into()),
    ]))
}

#[test]
fn a_tick_names_its_chat_and_the_prompt_entry() {
    assert_eq!(
        tick("41"),
        Some(Action::ChatTurn {
            pane: "w1:p1".into(),
            entry: 41,
        })
    );
}

#[test]
fn a_tick_without_a_prompt_entry_asks_nothing() {
    assert_eq!(tick("not an id"), None);
    assert_eq!(tick("-1"), None);
    assert_eq!(action(&Data::map([("chat_turn", "w1:p1".into())])), None);
}
