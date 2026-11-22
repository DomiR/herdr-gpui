use super::*;
use crate::bench::wire::Server;

#[test]
fn every_message_survives_encoding() {
    for message in [
        Message::Echo(vec![1, 2, 3]),
        Message::Sink(vec![9; 100]),
        Message::Tally,
        Message::Tallied(1 << 40),
        Message::Pull {
            bytes: 70_000,
            size: 4095,
        },
        Message::Data(Vec::new()),
    ] {
        assert_eq!(Message::decode(&message.encode()), Some(message));
    }
}

#[test]
fn malformed_messages_are_not_read() {
    assert_eq!(Message::decode(&[]), None);
    assert_eq!(Message::decode(b"x"), None);
    // A tally carries nothing; a tallied count and a pull carry exactly
    // their numbers.
    assert_eq!(Message::decode(b"t1"), None);
    assert_eq!(Message::decode(b"T1234"), None);
    assert_eq!(Message::decode(b"p12345"), None);
    let mut oversized = vec![b'e'];
    oversized.resize(MAX_BODY + 2, 0);
    assert_eq!(Message::decode(&oversized), None);
}

#[test]
fn bodies_never_outgrow_a_frame() {
    let encoded = Message::Sink(vec![0; MAX_BODY * 2]).encode();
    assert_eq!(encoded.len(), crate::framing::MAX_MESSAGE);
}

#[test]
fn the_server_echoes_sinks_tallies_and_serves_pulls() {
    let mut server = Server::default();
    let echo = Message::Echo(b"ping".to_vec()).encode();
    assert_eq!(server.reply(&echo), vec![echo]);
    assert!(
        server
            .reply(&Message::Sink(vec![0; 1000]).encode())
            .is_empty()
    );
    assert!(
        server
            .reply(&Message::Sink(vec![0; 24]).encode())
            .is_empty()
    );
    assert_eq!(
        server.reply(&Message::Tally.encode()),
        vec![Message::Tallied(1024).encode()]
    );
    // A tally counts only what came since the last.
    assert_eq!(
        server.reply(&Message::Tally.encode()),
        vec![Message::Tallied(0).encode()]
    );
    let replies = server.reply(&Message::Pull { bytes: 10, size: 4 }.encode());
    let sizes: Vec<usize> = replies
        .iter()
        .map(|reply| match Message::decode(reply) {
            Some(Message::Data(data)) => data.len(),
            other => panic!("not data: {other:?}"),
        })
        .collect();
    assert_eq!(sizes, [4, 4, 2]);
    assert!(server.reply(b"garbage").is_empty());
    assert!(server.reply(&Message::Data(vec![1]).encode()).is_empty());
}

#[test]
fn a_pull_is_bounded() {
    let mut server = Server::default();
    let replies = server.reply(
        &Message::Pull {
            bytes: u32::MAX,
            size: u16::MAX,
        }
        .encode(),
    );
    let total: usize = replies.iter().map(|reply| reply.len() - 1).sum();
    assert_eq!(
        total,
        usize::try_from(crate::bench::wire::MAX_PULL).unwrap()
    );
    assert!(
        replies
            .iter()
            .all(|reply| reply.len() <= crate::framing::MAX_MESSAGE)
    );
    // A zero size still makes progress.
    let replies = server.reply(&Message::Pull { bytes: 3, size: 0 }.encode());
    assert_eq!(replies.len(), 3);
}
