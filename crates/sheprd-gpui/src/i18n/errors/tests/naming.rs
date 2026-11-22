use super::*;
use crate::naming::Error;

#[test]
fn every_naming_variant_has_a_message() {
    assert_localized(&[
        Error::Blocked {
            agent: "Codex".into(),
        },
        Error::Core(sheprd_core_client::Error::Lost),
        Error::Empty,
        Error::Gone,
        Error::NoName {
            agent: "Claude".into(),
        },
        Error::Script(sheprd_client::Error::ResponseType),
        Error::TimedOut {
            agent: "Claude".into(),
        },
    ]);
}

#[test]
fn wrapped_failures_keep_their_own_message() {
    assert_localized(&[
        Error::Claude(sheprd_client::one_shot::Error::NoAgent),
        Error::Host(crate::pane_move::Error::UnsupportedHost),
    ]);
}
