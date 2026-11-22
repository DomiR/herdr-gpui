//! The serving side under load: tiny pulls stay bounded, and computers
//! running at once keep their own counts.

use super::*;
use crate::bench::wire::{MAX_PULL, MAX_PULL_REPLIES, Server};

fn pulled(replies: &[Vec<u8>]) -> usize {
    replies
        .iter()
        .map(|reply| match Message::decode(reply) {
            Some(Message::Data(data)) => data.len(),
            other => panic!("not data: {other:?}"),
        })
        .sum()
}

#[test]
fn a_pull_of_tiny_bodies_is_answered_in_a_bounded_number_of_replies() {
    let mut server = Server::default();
    let replies = server.reply(
        &Message::Pull {
            bytes: MAX_PULL,
            size: 1,
        }
        .encode(),
    );
    assert!(
        replies.len() <= MAX_PULL_REPLIES,
        "{} replies",
        replies.len()
    );
    assert_eq!(pulled(&replies), usize::try_from(MAX_PULL).unwrap());
    assert!(
        replies
            .iter()
            .all(|reply| reply.len() <= crate::framing::MAX_MESSAGE)
    );
}

#[test]
fn computers_uploading_at_once_are_tallied_apart() {
    let mut servers = Servers::default();
    assert!(
        servers
            .reply("A", &Message::Sink(vec![0; 100]).encode())
            .is_empty()
    );
    assert!(
        servers
            .reply("B", &Message::Sink(vec![0; 7]).encode())
            .is_empty()
    );
    assert_eq!(
        servers.reply("A", &Message::Tally.encode()),
        vec![Message::Tallied(100).encode()]
    );
    assert_eq!(
        servers.reply("B", &Message::Tally.encode()),
        vec![Message::Tallied(7).encode()]
    );
    // A computer that leaves takes its count with it.
    servers.reply("A", &Message::Sink(vec![0; 5]).encode());
    servers.forget("A");
    assert_eq!(
        servers.reply("A", &Message::Tally.encode()),
        vec![Message::Tallied(0).encode()]
    );
}
