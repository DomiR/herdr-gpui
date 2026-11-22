//! The host builds' `sheprd-acp` (schema 5): what only its family checks.

use super::{packed::VERSION, *};
use crate::release::host_chat::CHATS;

#[test]
fn the_archive_is_named_for_the_release() {
    assert_eq!(
        CHATS.archive_name(VERSION),
        "sheprd-acp-20261108.3-linux-musl.tar.gz"
    );
}

#[test]
fn chats_ship_for_the_host_build_targets() {
    assert_eq!(CHATS.targets, HOST_TARGETS);
    assert_eq!(CHATS.schema, 5);
}
