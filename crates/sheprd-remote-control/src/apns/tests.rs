#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::push::Chat;

mod alert;
mod clear;
mod decide;
mod delivery;
mod notify_key;
mod pacer;
mod payload;
mod per_host;
mod push_relay_revoke;
mod recipient;
mod redact;
mod relayed;
mod relayed_fixture;
mod repeat;
mod terminal;
mod test_push;
mod tokens;

const CHAT: Chat<'static> = Chat {
    agent: "Claude",
    pane: "w1-p3",
    title: "remote plan",
    workspace: "herdr-gpui",
};

fn token(byte: u8) -> Recipient {
    DeviceToken::parse(&format!("{byte:02x}").repeat(32))
        .unwrap()
        .into()
}
