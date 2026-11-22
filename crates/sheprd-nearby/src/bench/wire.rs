//! The harness's messages, carried as whole `pair` frames: a tag byte,
//! then a body. The serving side answers each with [`Server::reply`].

use crate::framing::MAX_MESSAGE;
use std::collections::HashMap;

/// The most a [`Message::Pull`] may ask for, so a stray frame cannot keep
/// the radio busy for long.
pub(crate) const MAX_PULL: u32 = 4 * 1024 * 1024;
/// The most [`Message::Data`] replies one pull is answered with: a tiny
/// `size` is raised so a seven-byte request cannot queue millions of frames.
pub(crate) const MAX_PULL_REPLIES: usize = 2048;
/// The largest body a message carries beside its tag.
pub(crate) const MAX_BODY: usize = MAX_MESSAGE - 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Message {
    /// Answered with itself.
    Echo(Vec<u8>),
    /// Counted and not answered: the upload.
    Sink(Vec<u8>),
    /// Answered with [`Message::Tallied`]: the bytes sunk since the last.
    Tally,
    Tallied(u64),
    /// Answered with `bytes` in [`Message::Data`] bodies of `size`: the
    /// download.
    Pull {
        bytes: u32,
        size: u16,
    },
    Data(Vec<u8>),
}

const ECHO: u8 = b'e';
const SINK: u8 = b's';
const TALLY: u8 = b't';
const TALLIED: u8 = b'T';
const PULL: u8 = b'p';
const DATA: u8 = b'd';

impl Message {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let (tag, body): (u8, &[u8]) = match self {
            Self::Echo(body) => (ECHO, body),
            Self::Sink(body) => (SINK, body),
            Self::Data(body) => (DATA, body),
            Self::Tally => (TALLY, &[]),
            Self::Tallied(bytes) => return [&[TALLIED][..], &bytes.to_be_bytes()].concat(),
            Self::Pull { bytes, size } => {
                return [&[PULL][..], &bytes.to_be_bytes(), &size.to_be_bytes()].concat();
            }
        };
        let body = &body[..body.len().min(MAX_BODY)];
        [&[tag][..], body].concat()
    }

    /// Reads a message; anything else is `None`.
    pub(crate) fn decode(message: &[u8]) -> Option<Self> {
        let (&tag, body) = message.split_first()?;
        if body.len() > MAX_BODY {
            return None;
        }
        Some(match tag {
            ECHO => Self::Echo(body.to_vec()),
            SINK => Self::Sink(body.to_vec()),
            DATA => Self::Data(body.to_vec()),
            TALLY if body.is_empty() => Self::Tally,
            TALLIED => Self::Tallied(u64::from_be_bytes(body.try_into().ok()?)),
            PULL => {
                let [b0, b1, b2, b3, s0, s1] = body.try_into().ok()?;
                Self::Pull {
                    bytes: u32::from_be_bytes([b0, b1, b2, b3]),
                    size: u16::from_be_bytes([s0, s1]),
                }
            }
            _ => return None,
        })
    }
}

/// The serving side: what it answers each message with.
#[derive(Debug, Default)]
pub(crate) struct Server {
    sunk: u64,
}

impl Server {
    /// The answers to `message`, in order; nothing for what it does not
    /// read.
    pub(crate) fn reply(&mut self, message: &[u8]) -> Vec<Vec<u8>> {
        match Message::decode(message) {
            Some(Message::Echo(_)) => vec![message.to_vec()],
            Some(Message::Sink(body)) => {
                self.sunk += body.len() as u64;
                Vec::new()
            }
            Some(Message::Tally) => {
                let sunk = std::mem::take(&mut self.sunk);
                vec![Message::Tallied(sunk).encode()]
            }
            Some(Message::Pull { bytes, size }) => {
                let mut left = usize::try_from(bytes.min(MAX_PULL)).unwrap_or(0);
                let size = usize::from(size)
                    .max(left.div_ceil(MAX_PULL_REPLIES))
                    .clamp(1, MAX_BODY);
                let mut replies = Vec::with_capacity(left.div_ceil(size));
                while left > 0 {
                    let take = left.min(size);
                    replies.push(Message::Data(vec![0xA5; take]).encode());
                    left -= take;
                }
                replies
            }
            Some(Message::Tallied(_) | Message::Data(_)) | None => Vec::new(),
        }
    }
}

/// One [`Server`] for each connected computer, so two running at once
/// each tally only their own upload. A computer's count goes with it.
#[derive(Debug, Default)]
pub(crate) struct Servers(HashMap<String, Server>);

impl Servers {
    /// The answers to `message` from the computer `central`.
    pub(crate) fn reply(&mut self, central: &str, message: &[u8]) -> Vec<Vec<u8>> {
        if let Some(server) = self.0.get_mut(central) {
            return server.reply(message);
        }
        self.0.entry(central.to_owned()).or_default().reply(message)
    }

    /// Forgets the computer `central`, which disconnected.
    pub(crate) fn forget(&mut self, central: &str) {
        self.0.remove(central);
    }
}
