//! Bounded plain text from a transcript, for an agent run once on its own
//! to read: a phone's row summary, a chat's title, or a workspace's name.

use crate::{EntryKind, Transcript};

/// The most of the transcript [`transcript`] keeps, in characters, newest kept.
pub const MAX_INPUT_CHARS: usize = 12_000;
/// The most of one message [`transcript`] keeps, in characters.
const MAX_ENTRY_CHARS: usize = 2_000;

/// `text` on one line, its runs of whitespace one space, cut to `limit`
/// characters with an ellipsis.
pub fn one_line(text: &str, limit: usize) -> String {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        return line;
    }
    let mut cut: String = line.chars().take(limit.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

/// The person's prompts and the agent's answers, oldest first, as much of
/// the end as fits in [`MAX_INPUT_CHARS`]; empty when nothing was said yet.
pub fn transcript(transcript: &Transcript) -> String {
    let mut parts = Vec::new();
    let mut left = MAX_INPUT_CHARS;
    for entry in transcript.entries.iter().rev() {
        let (who, text) = match &entry.kind {
            EntryKind::User { text, .. } => ("Person", text),
            EntryKind::Agent { text } => ("Agent", text),
            _ => continue,
        };
        let line = format!("{who}: {}", one_line(text, MAX_ENTRY_CHARS));
        let size = line.chars().count();
        if size > left {
            break;
        }
        left -= size;
        parts.push(line);
    }
    parts.reverse();
    parts.join("\n\n")
}
