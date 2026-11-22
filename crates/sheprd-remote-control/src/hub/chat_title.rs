//! A name for a chat: Claude, run once on its own as for a row summary
//! ([`super::row_summary`]), reads the end of the transcript and answers a
//! short title. Nothing is sent to the chat,
//! so its session and context are untouched; the phone renames the chat.

use super::row_summary;
use sheprd_chat_protocol::naming;
use sheprd_client::one_shot;

/// Blocking: the title for `input`, the excerpt of a transcript.
pub(crate) fn run(input: &str) -> Result<String, row_summary::Error> {
    let answer = one_shot::ask(
        naming::TITLE_PROMPT,
        input,
        &std::sync::atomic::AtomicBool::new(false),
    )?;
    naming::title_from(&answer).ok_or(one_shot::Error::Empty.into())
}
