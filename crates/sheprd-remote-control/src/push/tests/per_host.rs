//! Pushes from several computers to one phone: the same pane on two of
//! them never replaces the other's undelivered push.

use super::*;

const HOST: &str = "9E92A0DF1CE0967EC34C0C0146005FBC51654330E7D26EC9B5805055755E24F6";
const OTHER_HOST: &str = "9E92A0DF1CE0967E0000000000000000000000000000000000000000000000AA";

#[test]
fn the_same_pane_on_two_computers_has_two_topics() {
    assert_ne!(
        Topic::from_tag(HOST, "chat-w1:p2"),
        Topic::from_tag(OTHER_HOST, "chat-w1:p2")
    );
    assert_eq!(
        Topic::from_tag(HOST, "chat-w1:p2"),
        Topic::from_tag(HOST, "chat-w1:p2"),
        "a resend of one chat still replaces its push"
    );
    // The separator keeps where the computer ends from moving.
    assert_ne!(Topic::from_tag("AB", "C"), Topic::from_tag("A", "BC"));
}
