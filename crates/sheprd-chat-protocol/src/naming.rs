//! Asking an agent for a short name for a chat, tab, or workspace, two ways:
//!
//! - Claude run once on its own reads an excerpt ([`TITLE_PROMPT`]) and
//!   answers a title; nothing reaches the chat or terminal named.
//! - The agent working there is asked in its own conversation
//!   ([`ask_prompt`]), and its answer is read back: from a chat's
//!   transcript ([`answer`]), or from a terminal's text ([`marked`]), where
//!   it starts with [`MARK`] so it can be told from everything else shown.
//!
//! No agent can rename what Sheprd keeps, so the client renames it with the
//! answer, after the person has seen it.

use crate::{EntryId, EntryKind, Transcript, TurnEnd};

/// The longest name kept, in characters; a tab or a phone's title bar
/// shows about this many.
pub const MAX_CHARS: usize = 40;

/// What Claude run on its own is asked; the excerpt follows on its input.
/// The limits are spelled out because agents otherwise answer a sentence.
pub const TITLE_PROMPT: &str = "The input is the end of the work in a terminal tab: \
conversations between a person and a coding agent, or what a terminal showed. Reply with a \
short title for it, as a tab would show: 2 to 5 plain words, at most 40 characters, no \
quotes, punctuation, emoji, Markdown, or explanation.";

/// What an agent's answer to [`ask_prompt`] starts with.
pub const MARK: &str = "NAME:";

/// What is being named.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Subject {
    Tab,
    Workspace,
}

impl Subject {
    fn noun(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Workspace => "workspace",
        }
    }
}

/// What the agent working in `subject`, now named `current`, is asked.
/// `request` sets this ask apart from earlier ones still shown in a
/// terminal ([`terminal_answer`]). The prompt never holds [`MARK`] itself,
/// so its echo is not taken for the answer.
pub fn ask_prompt(subject: Subject, current: &str, request: u64) -> String {
    let noun = subject.noun();
    // A name holding the mark would echo as an answer.
    let current = crate::excerpt::one_line(&current.replace(MARK, ""), MAX_CHARS * 2);
    format!(
        "{} suggest a short name for this {noun}, based on the work in this conversation. \
         It is currently named \"{current}\". Reply with one line only: the word NAME in \
         capitals, a colon, a space, then the name: 2 to 5 plain words, at most {MAX_CHARS} \
         characters, no quotes, punctuation, emoji, or explanation. Do not run tools or \
         change any files.",
        request_tag(request)
    )
}

/// How [`ask_prompt`] starts, at the start of the line a terminal echoes
/// it on, where no wrapping splits it.
fn request_tag(request: u64) -> String {
    format!("Request {request}:")
}

/// The name a terminal's `text` shows in answer to the [`ask_prompt`] of
/// `request`: the last one [`marked`] after that prompt's echo. `None` until
/// the agent has echoed the prompt and answered it.
pub fn terminal_answer(text: &str, request: u64) -> Option<String> {
    let asked = text.rfind(&request_tag(request))?;
    marked(&text[asked..]).pop()
}

/// How the agent answered [`ask_prompt`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Answer {
    Name(String),
    /// The turn ended without one: stopped, refused, failed, or empty.
    NoName,
}

/// The answer to `prompt` in `transcript`, once the turn that answers it has
/// ended; `None` while it is still answering, or when the prompt is not in
/// the transcript (yet). Only a prompt entered after `after`, the
/// transcript's last entry when it was sent, counts, so a retry waits for
/// its own answer behind a busy agent.
pub fn answer(prompt: &str, transcript: &Transcript, after: Option<EntryId>) -> Option<Answer> {
    let asked = transcript.entries.iter().rposition(|entry| {
        after.is_none_or(|after| entry.id > after)
            && matches!(&entry.kind, EntryKind::User { text, .. } if text.trim() == prompt.trim())
    })?;
    let rest = &transcript.entries[asked + 1..];
    let ended = rest
        .iter()
        .position(|entry| matches!(entry.kind, EntryKind::Turn(_)))?;
    let EntryKind::Turn(summary) = &rest[ended].kind else {
        return None;
    };
    if summary.end != TurnEnd::Done {
        return Some(Answer::NoName);
    }
    let reply: String = rest[..ended]
        .iter()
        .filter_map(|entry| match &entry.kind {
            EntryKind::Agent { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(title_from(line).map_or(Answer::NoName, Answer::Name))
}

/// The names a terminal's `text` shows in answer to [`ask_prompt`], oldest
/// first: each line that starts with [`MARK`] once what the agent's view
/// draws before it (a bullet, a box edge) is set aside.
pub fn marked(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches(|c: char| !c.is_alphanumeric());
            let rest = line.strip_prefix(MARK)?;
            // A box the agent draws its reply in closes on the right.
            title_from(rest.trim_end_matches(['│', '|', '┃']))
        })
        .collect()
}

/// The answer without the quotes, emphasis, label, or final period agents
/// add anyway, cut on a word to [`MAX_CHARS`].
pub fn title_from(answer: &str) -> Option<String> {
    const WRAPPING: &[char] = &['"', '\'', '`', '*', '_', '#', '“', '”', '‘', '’', ' ', '.'];
    let mut title = answer.trim_matches(WRAPPING);
    for label in ["Title:", "title:", "Name:", "name:", MARK] {
        if let Some(rest) = title.strip_prefix(label) {
            title = rest.trim_matches(WRAPPING);
        }
    }
    let mut words = title.split_whitespace();
    let mut kept: String = words.next()?.chars().take(MAX_CHARS).collect();
    for word in words {
        if kept.chars().count() + 1 + word.chars().count() > MAX_CHARS {
            break;
        }
        kept.push(' ');
        kept.push_str(word);
    }
    Some(kept)
}
