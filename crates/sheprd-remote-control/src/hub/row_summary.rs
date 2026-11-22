//! A smarter line for a chat's row: Claude, run once on its own, reads the
//! end of the chat's transcript and says where it stands.
//! The chat's session, history, and context are untouched, as nothing is
//! sent to it; the summary is the daemon's and lives with the chat's link.

mod error;

pub(crate) use error::Error;

use serde::{Deserialize, Serialize};
use sheprd_chat_protocol::Transcript;
use sheprd_client::one_shot;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Summaries running at once across every chat; one more is refused.
const MAX_RUNNING: usize = 3;

/// What the agent is asked; the transcript follows on its input.
const PROMPT: &str = "The input is the end of a conversation between a person and a coding \
agent. Summarize where it stands for one row of a list on a phone: at most two short \
sentences, plain text, no Markdown, no quotes, no preamble. Say what is being worked on and \
what is done, next, or blocked.";

/// A chat's summary as its row shows it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum RowSummary {
    /// The agent is reading the chat.
    Pending {
        since_ms: u64,
    },
    Ready {
        text: String,
        at_ms: u64,
    },
    /// Why there is none, as the person reads it.
    Failed {
        reason: String,
        at_ms: u64,
    },
}

/// The person's prompts and the agent's answers, oldest first, as much of
/// the end as fits; empty when nothing was said yet.
pub(crate) fn excerpt_of(transcript: &Transcript) -> String {
    sheprd_chat_protocol::excerpt::transcript(transcript)
}

/// Counts a running summary for [`MAX_RUNNING`] until dropped.
pub(crate) struct Slot;

static RUNNING: AtomicUsize = AtomicUsize::new(0);

impl Slot {
    pub(crate) fn take() -> Result<Self, Error> {
        RUNNING
            .try_update(Ordering::AcqRel, Ordering::Acquire, |running| {
                (running < MAX_RUNNING).then_some(running + 1)
            })
            .map(|_| Self)
            .map_err(|_| Error::Busy)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Blocking: the summary of `input`.
pub(crate) fn run(input: &str) -> Result<String, Error> {
    // The phone's request does not wait for the summary, so nothing stops it.
    Ok(one_shot::ask(
        PROMPT,
        input,
        &std::sync::atomic::AtomicBool::new(false),
    )?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
