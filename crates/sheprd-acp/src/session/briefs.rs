//! Marking the prompts another agent wrote (`crate::briefs`): the brief the
//! chat was started with, and on a continued session the replayed prompts
//! whose text matches one kept for it.

use std::path::PathBuf;

use sheprd_chat_protocol::{Brief, Change, CommandId, EntryId, EntryKind, MAX_BRIEFS, NoticeLevel};

use super::{Session, Update};
use crate::briefs::{self, Known};

/// The briefs this session looks for.
#[derive(Debug, Default)]
pub(super) struct Briefs {
    /// Where they are kept; `None` keeps nothing, as in tests.
    dir: Option<PathBuf>,
    known: Vec<Known>,
    /// Sent this run and not yet kept for the agent's session.
    unsaved: Vec<u64>,
}

impl<R: FnMut(Update)> Session<R> {
    /// Keeps briefs in `dir`, and marks the continued session's earlier
    /// prompts that were briefs.
    pub(crate) fn keep_briefs(&mut self, dir: PathBuf) {
        if let Some(session) = &self.config.load {
            self.briefs.known = briefs::load(&dir, session);
        }
        self.briefs.dir = Some(dir);
        let ids: Vec<EntryId> = self
            .transcript
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect();
        for id in ids {
            if let Some(brief) = self.brief_of(id) {
                if self.transcript.briefs.len() >= MAX_BRIEFS {
                    break;
                }
                self.transcript.briefs.push(brief);
            }
        }
    }

    /// Sends `brief` as the first prompt, like a client's, and marks its
    /// entry once the prompt shows.
    pub(crate) fn brief(&mut self, brief: briefs::Brief) {
        let known = Known::of(&brief.text, brief.from);
        self.briefs.known.retain(|kept| kept.sum != known.sum);
        self.briefs.known.push(known);
        self.briefs.unsaved.push(known.sum);
        if let Err(rejection) = self.prompt(CommandId::random(), brief.text, Vec::new(), None) {
            self.briefs.unsaved.retain(|sum| *sum != known.sum);
            self.notice(
                NoticeLevel::Warning,
                format!("The brief this chat was started with was not sent: {rejection:?}"),
            );
        }
    }

    /// Marks the user entry `change` pushed or added to when its text is a
    /// known brief's.
    pub(super) fn find_brief(&mut self, change: &Change) {
        if self.briefs.known.is_empty() {
            return;
        }
        let id = match change {
            Change::Push { entry } | Change::Replace { entry } => entry.id,
            Change::Append { id, .. } => *id,
            _ => return,
        };
        let Some(brief) = self.brief_of(id) else {
            return;
        };
        if self.transcript.brief(id) == Some(&brief) {
            return;
        }
        self.emit(Change::Brief { brief });
        self.save_brief(id);
    }

    /// The brief entry `id` is, if it is a prompt whose text is a known one's.
    fn brief_of(&self, id: EntryId) -> Option<Brief> {
        let EntryKind::User { text, .. } = &self.transcript.entry(id)?.kind else {
            return None;
        };
        let sum = briefs::checksum(text);
        let known = self.briefs.known.iter().find(|known| known.sum == sum)?;
        Some(Brief {
            entry: id,
            from: known.from,
        })
    }

    /// Keeps a brief sent this run for the agent's session, so continuing it
    /// marks the prompt again.
    fn save_brief(&mut self, id: EntryId) {
        let Some(EntryKind::User { text, .. }) = self.transcript.entry(id).map(|entry| &entry.kind)
        else {
            return;
        };
        let sum = briefs::checksum(text);
        let (Some(dir), Some(session)) = (&self.briefs.dir, &self.acp_session) else {
            return;
        };
        if !self.briefs.unsaved.contains(&sum) {
            return;
        }
        let Some(known) = self
            .briefs
            .known
            .iter()
            .find(|known| known.sum == sum)
            .copied()
        else {
            return;
        };
        let saved = briefs::save(dir, &session.0, known);
        self.briefs.unsaved.retain(|unsaved| *unsaved != sum);
        if let Err(error) = saved {
            self.notice(
                NoticeLevel::Warning,
                format!("A continued chat will show the brief as a prompt: {error}"),
            );
        }
    }
}
