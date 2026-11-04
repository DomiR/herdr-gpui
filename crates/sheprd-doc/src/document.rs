//! The CRDT document: one Loro text for one Markdown file.
//!
//! Three kinds of writer change it. The person in the editor edits the
//! document's own peer, the only one its undo manager records, so undo never
//! reverts anyone else. An agent's command edits a fork at the latest
//! version as that agent's own peer, imported like a remote change. A new
//! text on disk is diffed against the text last synced with the disk and
//! applied on a fork at that synced version, so it merges with whatever
//! the user and agents did since rather than replacing it.

use crate::{
    Conflict, Edit, Error, MAX_BYTES, Result, Version, diff, section, todo,
    version::{self, History},
};
use loro::{
    ExportMode, Frontiers, LoroDoc, LoroText, PeerID, UndoManager,
    cursor::{Cursor, PosType, Side},
};
use std::{collections::HashMap, ops::Range};

/// The container every document keeps its text in.
const TEXT: &str = "markdown";
/// Typing within this many milliseconds undoes as one step.
const UNDO_MERGE_MS: i64 = 600;
/// The most writes in flight remembered.
const MAX_WRITING: usize = 8;
/// The most undo steps kept.
const UNDO_STEPS: usize = 500;

/// Who made a change the user did not type: the pane an agent runs in, and
/// what the editor calls it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Author {
    pub pane: String,
    pub label: String,
}

/// Who made a version: the person in the editor, an agent through a
/// command, or another program writing the file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    User,
    Agent(Author),
    Disk,
}

/// What changed in a document since a version, as `doc diff` and `doc
/// watch` report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changes {
    pub from: Version,
    pub to: Version,
    /// Who made the versions in between, each once.
    pub by: Vec<Origin>,
    /// A unified diff (of the section alone, when one was named); `None`
    /// when too large.
    pub diff: Option<String>,
    /// The tasks that changed, by the document's own numbering.
    pub todos: Vec<todo::TodoChange>,
}

/// The text the file held when the document last read or wrote it, and the
/// document's version then: a new disk text is a change from this.
#[derive(Clone, Debug)]
pub struct Synced {
    frontiers: Frontiers,
    text: String,
    version: Version,
}

impl Synced {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The newest version the disk is known to hold every change of.
    pub fn version(&self) -> Version {
        self.version
    }
}

/// What merging a disk text did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Merge {
    /// The disk held the synced or the current text: nothing to apply. This
    /// is how the document's own writes come back without echoing.
    Unchanged,
    /// The disk change is merged in; these ranges of the current text hold
    /// what it inserted.
    Merged { inserted: Vec<Range<usize>> },
}

/// What an agent made its change against: the version it read, and the
/// section it works in, if it named one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope<'a> {
    pub version: Version,
    pub section: Option<&'a str>,
}

/// An agent's change, applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The version the change made.
    pub version: Version,
    /// The ranges of the new text it inserted.
    pub inserted: Vec<Range<usize>>,
    /// Set when the document had moved on from the version named but not
    /// in the named section: that version, and a diff of what changed in
    /// the other sections since (`None` when too large).
    pub others: Option<(Version, Option<String>)>,
}

/// A place in the text that moves with the edits around it, whoever makes
/// them: a selection end, or one end of a highlight.
#[derive(Clone, Debug)]
pub struct Anchor(Cursor);

pub struct Document {
    doc: LoroDoc,
    text: LoroText,
    undo: UndoManager,
    /// The text as of the last change, kept so readers need not rebuild it.
    current: String,
    synced: Synced,
    /// Texts handed out to be written and not yet confirmed, oldest first:
    /// a poll reading one of them back is the document's own write.
    writing: Vec<Synced>,
    /// Each agent's own peer, by pane.
    agents: HashMap<String, PeerID>,
    /// Bumped by every change, from any writer.
    version: Version,
    /// Bumped by every compaction.
    generation: u64,
    /// Where recent versions are in the history.
    history: History,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("len", &self.current.len())
            .field("version", &self.version)
            .field("agents", &self.agents.len())
            .finish_non_exhaustive()
    }
}

impl Document {
    /// A document holding `text`, as just read from disk: synced, with
    /// nothing to undo, at version 1.
    pub fn new(text: &str) -> Result<Self> {
        Self::at(text, Version::FIRST)
    }

    /// [`Self::new`], numbered `version`: a file opened again in the same
    /// session carries on counting.
    pub fn at(text: &str, version: Version) -> Result<Self> {
        check_len(text.len())?;
        let doc = LoroDoc::new();
        let body = doc.get_text(TEXT);
        body.insert_utf8(0, text)?;
        doc.commit();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(UNDO_MERGE_MS);
        undo.set_max_undo_steps(UNDO_STEPS);
        let mut history = History::default();
        history.record(version, doc.oplog_frontiers(), None);
        Ok(Self {
            synced: Synced {
                frontiers: doc.oplog_frontiers(),
                text: text.to_owned(),
                version,
            },
            doc,
            text: body,
            undo,
            current: text.to_owned(),
            writing: Vec::new(),
            agents: HashMap::new(),
            version,
            generation: 0,
            history,
        })
    }

    pub fn text(&self) -> &str {
        &self.current
    }

    /// The version number of the current text.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Whether `expected` is the current version; if not, the conflict to
    /// report, with the diff from that version's text when it is recent.
    pub fn check_version(&self, expected: Version) -> std::result::Result<(), Box<Conflict>> {
        if expected == self.version {
            return Ok(());
        }
        let diff = self
            .text_at(expected)
            .and_then(|old| version::unified(&old, &self.current, expected, self.version));
        Err(Box::new(Conflict {
            expected,
            current: self.version,
            diff,
        }))
    }

    /// The text last synced with the disk.
    pub fn synced(&self) -> &Synced {
        &self.synced
    }

    /// Whether the text differs from what the disk was last known to hold.
    pub fn dirty(&self) -> bool {
        self.current != self.synced.text
    }

    /// What to write now: the current text and its version, to hand back
    /// to [`Self::wrote`] once the write succeeded. Until then, reading it
    /// back from disk is recognized as this write.
    pub fn to_write(&mut self) -> Synced {
        let write = self.snapshot();
        if self.writing.len() == MAX_WRITING {
            self.writing.remove(0);
        }
        self.writing.push(write.clone());
        write
    }

    /// Records that the disk now holds `written`.
    pub fn wrote(&mut self, written: Synced) {
        if let Some(at) = self.writing.iter().position(|w| w.text == written.text) {
            self.writing.drain(..=at);
        }
        self.synced = written;
    }

    /// Records that a write handed out by [`Self::to_write`] did not happen.
    pub fn not_written(&mut self, write: &Synced) {
        self.writing.retain(|w| w.text != write.text);
    }

    fn snapshot(&self) -> Synced {
        Synced {
            frontiers: self.doc.oplog_frontiers(),
            text: self.current.clone(),
            version: self.version,
        }
    }

    /// The user's own edits, in the current text's byte offsets, as one
    /// undoable change.
    pub fn edit(&mut self, edits: &[Edit]) -> Result<()> {
        let sorted = self.check(edits)?;
        if sorted.is_empty() {
            return Ok(());
        }
        apply(&self.text, &sorted)?;
        self.doc.commit();
        self.refresh(Origin::User);
        Ok(())
    }

    /// Ends the current undo step, so the next edit undoes on its own even
    /// when typed right after.
    pub fn break_undo(&mut self) {
        let _ = self.undo.record_new_checkpoint();
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// Undoes the user's last step, never an agent's or the disk's. The
    /// range of the current text it changed, if it changed any.
    pub fn undo(&mut self) -> Result<Option<Range<usize>>> {
        let before = self.current.clone();
        self.undo.undo()?;
        Ok(self.after_history(&before))
    }

    pub fn redo(&mut self) -> Result<Option<Range<usize>>> {
        let before = self.current.clone();
        self.undo.redo()?;
        Ok(self.after_history(&before))
    }

    fn after_history(&mut self, before: &str) -> Option<Range<usize>> {
        self.refresh(Origin::User);
        changed_range(before, &self.current)
    }

    /// An agent's edits, made by `make` from the current text, as the
    /// agent's own peer. They never enter the user's undo history.
    ///
    /// The change must be made against the current version, or else name a
    /// section whose text is the same now as at the version it names;
    /// otherwise nothing changes and the error is [`Error::Conflict`] with
    /// the diff since that version.
    pub fn agent_edit(
        &mut self,
        author: &Author,
        scope: Scope<'_>,
        make: impl FnOnce(&str) -> Result<Vec<Edit>>,
    ) -> Result<Applied> {
        let others = match self.check_version(scope.version) {
            Ok(()) => None,
            Err(conflict) => {
                let unchanged = scope.section.is_some_and(|id| {
                    let old = self.text_at(scope.version);
                    let then = old.as_deref().and_then(|old| section_text(old, id));
                    then.is_some() && then == section_text(&self.current, id)
                });
                if !unchanged {
                    return Err(Error::Conflict(conflict));
                }
                Some((scope.version, conflict.diff))
            }
        };
        let edits = make(&self.current)?;
        let sorted = self.check(&edits)?;
        if sorted.is_empty() {
            return Ok(Applied {
                version: self.version,
                inserted: Vec::new(),
                others,
            });
        }
        let fork = self.doc.fork();
        match self.agents.get(&author.pane) {
            Some(peer) => fork.set_peer_id(*peer)?,
            None => {
                self.agents.insert(author.pane.clone(), fork.peer_id());
            }
        }
        let from = fork.oplog_vv();
        let inserted = apply(&fork.get_text(TEXT), &sorted)?;
        fork.commit();
        let updates = fork.export(ExportMode::updates_owned(from))?;
        self.doc.import(&updates)?;
        self.refresh(Origin::Agent(author.clone()));
        Ok(Applied {
            version: self.version,
            inserted,
            others,
        })
    }

    /// A unified diff of what changed since `version`: empty when nothing
    /// did, `None` when that version is forgotten or the diff too large.
    pub fn changes_since(&self, version: Version) -> Option<String> {
        if version == self.version {
            return Some(String::new());
        }
        let old = self.text_at(version)?;
        version::unified(&old, &self.current, version, self.version)
    }

    /// How many CRDT operations the document's history holds.
    pub fn history_len(&self) -> usize {
        self.doc.len_ops()
    }

    /// Drops the history, as comet rebuilds a document grown fat, when
    /// nothing waits to be written: the document starts again from its
    /// current text. (Loro's shallow snapshots cannot fork at the synced
    /// version, which merging a disk change needs.) The text, its version,
    /// and each agent's peer stay; undo and diffs from older versions start
    /// afresh, and anchors no longer resolve: holders take them again from
    /// the same byte offsets when [`Self::generation`] moves. Whether it did.
    pub fn compact(&mut self) -> Result<bool> {
        if self.dirty() || !self.writing.is_empty() {
            return Ok(false);
        }
        let doc = LoroDoc::new();
        let text = doc.get_text(TEXT);
        text.insert_utf8(0, &self.current)?;
        doc.commit();
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(UNDO_MERGE_MS);
        undo.set_max_undo_steps(UNDO_STEPS);
        let mut history = History::default();
        history.record(self.version, doc.oplog_frontiers(), None);
        self.synced = Synced {
            frontiers: doc.oplog_frontiers(),
            text: self.synced.text.clone(),
            version: self.synced.version,
        };
        self.doc = doc;
        self.text = text;
        self.undo = undo;
        self.history = history;
        self.generation += 1;
        Ok(true)
    }

    /// Bumped by [`Self::compact`]: anchors taken before it do not resolve.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What changed since `since`, in the section `section` (by ID) when
    /// one is named; `None` when that version is no longer known.
    pub fn changes(&self, since: Version, section: Option<&str>) -> Option<Changes> {
        let by = if since == self.version {
            Vec::new()
        } else {
            self.history.origins_since(since)?
        };
        let old = self.text_at(since)?;
        let new = self.current.as_str();
        let region = |text: &str| -> (String, Option<Range<usize>>) {
            match section.map(|id| section::find(text, id)) {
                None => (text.to_owned(), None),
                Some(Ok(found)) => (text[found.range].to_owned(), Some(found.lines)),
                Some(Err(_)) => (String::new(), Some(0..0)),
            }
        };
        let (old_part, old_lines) = region(&old);
        let (new_part, new_lines) = region(new);
        let diff = if old_part == new_part {
            Some(String::new())
        } else {
            version::unified(&old_part, &new_part, since, self.version)
        };
        let todos = todo::changes(&old, new)
            .into_iter()
            .filter(|change| {
                let (lines, line) = if change.kind == todo::ChangeKind::Removed {
                    (&old_lines, change.old_line)
                } else {
                    (&new_lines, change.line)
                };
                lines.as_ref().is_none_or(|lines| lines.contains(&line))
            })
            .collect();
        Some(Changes {
            from: since,
            to: self.version,
            by,
            diff,
            todos,
        })
    }

    /// The text at `version`, while the document still knows it.
    pub fn text_at(&self, version: Version) -> Option<String> {
        if version == self.version {
            return Some(self.current.clone());
        }
        let frontiers = self.history.find(version)?;
        let old = self.doc.fork_at(frontiers).ok()?;
        Some(old.get_text(TEXT).to_string())
    }

    /// Merges `disk`, the file's text now, with every change made since the
    /// document last synced with the disk. The disk change is applied at
    /// that synced version by a peer of its own, so text the user typed
    /// meanwhile stays where they typed it.
    pub fn merge_disk(&mut self, disk: &str) -> Result<Merge> {
        self.merge_disk_diffed(disk, None)
    }

    /// [`Self::merge_disk`] with the diff already worked out, off the UI
    /// thread, from `base` (the synced text the caller read): the edits are
    /// used when `base` is still the synced text, else worked out again.
    pub fn merge_disk_diffed(
        &mut self,
        disk: &str,
        diffed: Option<(&str, Vec<Edit>)>,
    ) -> Result<Merge> {
        check_len(disk.len())?;
        if disk == self.synced.text {
            return Ok(Merge::Unchanged);
        }
        // The document's own write, read back before it was confirmed.
        if let Some(at) = self.writing.iter().position(|w| w.text == disk) {
            self.synced = self.writing[at].clone();
            self.writing.drain(..=at);
            return Ok(Merge::Unchanged);
        }
        if disk == self.current {
            self.synced = self.snapshot();
            return Ok(Merge::Unchanged);
        }
        let fork = self.doc.fork_at(&self.synced.frontiers)?;
        let body = fork.get_text(TEXT);
        let edits = match diffed {
            Some((base, edits)) if base == self.synced.text => edits,
            _ => diff::edits(&body.to_string(), disk),
        };
        let from = fork.oplog_vv();
        let inserted = apply(&body, &edits)?;
        fork.commit();
        // The inserted characters by identity, found again once merged.
        let marks: Vec<(Cursor, Cursor, usize)> = inserted
            .iter()
            .filter_map(|range| {
                let last = disk[..range.end].chars().next_back()?;
                let first = unicode(&body, range.start)?;
                let end = unicode(&body, range.end - last.len_utf8())?;
                Some((
                    body.get_cursor(first, Side::Middle)?,
                    body.get_cursor(end, Side::Middle)?,
                    last.len_utf8(),
                ))
            })
            .collect();
        let updates = fork.export(ExportMode::updates_owned(from))?;
        self.doc.import(&updates)?;
        // The disk's text is no version of the document; it keeps the
        // last version known written, so no change is taken as written
        // before it is.
        self.synced = Synced {
            frontiers: fork.oplog_frontiers(),
            text: disk.to_owned(),
            version: self.synced.version,
        };
        self.refresh(Origin::Disk);
        let inserted = marks
            .iter()
            .filter_map(|(start, end, len)| {
                let start = self.resolve(&Anchor(start.clone()))?;
                let end = self.resolve(&Anchor(end.clone()))? + len;
                (start < end && end <= self.current.len()).then_some(start..end)
            })
            .collect();
        Ok(Merge::Merged { inserted })
    }

    /// An anchor at byte `offset`. `Side::Left` stays before text inserted
    /// right at it, `Side::Right` after.
    pub fn anchor(&self, offset: usize, after: bool) -> Option<Anchor> {
        let side = if after { Side::Right } else { Side::Left };
        let pos = unicode(&self.text, offset.min(self.current.len()))?;
        self.text.get_cursor(pos, side).map(Anchor)
    }

    /// Where `anchor` is now, in bytes.
    pub fn resolve(&self, anchor: &Anchor) -> Option<usize> {
        let found = self.doc.get_cursor_pos(&anchor.0).ok()?;
        self.text
            .convert_pos(found.current.pos, PosType::Unicode, PosType::Bytes)
            .map(|offset| offset.min(self.current.len()))
    }

    /// Checks `edits` against the current text and sorts them ascending.
    fn check(&self, edits: &[Edit]) -> Result<Vec<Edit>> {
        let mut sorted: Vec<Edit> = edits
            .iter()
            .filter(|edit| !(edit.range.is_empty() && edit.text.is_empty()))
            .cloned()
            .collect();
        sorted.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let mut previous_end = 0;
        let mut len = self.current.len();
        for edit in &sorted {
            let Range { start, end } = edit.range;
            if start < previous_end
                || start > end
                || end > self.current.len()
                || !self.current.is_char_boundary(start)
                || !self.current.is_char_boundary(end)
            {
                return Err(Error::BadRange { start, end });
            }
            previous_end = end;
            len = len - (end - start) + edit.text.len();
        }
        check_len(len)?;
        Ok(sorted)
    }

    /// Reads the text again after a change; a different text is the next
    /// version.
    fn refresh(&mut self, by: Origin) {
        let text = self.text.to_string();
        if text == self.current {
            return;
        }
        self.current = text;
        self.version = self.version.next();
        self.history
            .record(self.version, self.doc.oplog_frontiers(), Some(by));
    }
}

/// Applies ascending, checked `edits` to `text`, last first so earlier
/// offsets hold. The ranges of the new text they inserted.
fn apply(text: &LoroText, edits: &[Edit]) -> Result<Vec<Range<usize>>> {
    for edit in edits.iter().rev() {
        if !edit.range.is_empty() {
            text.delete_utf8(edit.range.start, edit.range.len())?;
        }
        if !edit.text.is_empty() {
            text.insert_utf8(edit.range.start, &edit.text)?;
        }
    }
    let mut shift: isize = 0;
    let mut inserted = Vec::new();
    for edit in edits {
        let start = edit.range.start.saturating_add_signed(shift);
        if !edit.text.is_empty() {
            inserted.push(start..start + edit.text.len());
        }
        shift += edit.text.len() as isize - edit.range.len() as isize;
    }
    Ok(inserted)
}

/// The text of section `id` in `text`, if it has one.
fn section_text<'a>(text: &'a str, id: &str) -> Option<&'a str> {
    let section = section::find(text, id).ok()?;
    text.get(section.range)
}

fn unicode(text: &LoroText, offset: usize) -> Option<usize> {
    text.convert_pos(offset, PosType::Bytes, PosType::Unicode)
}

fn check_len(len: usize) -> Result<()> {
    if len > MAX_BYTES {
        return Err(Error::TooLarge {
            len: len as u64,
            limit: MAX_BYTES,
        });
    }
    Ok(())
}

/// The range of `after` that differs from `before`, by common prefix and
/// suffix.
pub(crate) fn changed_range(before: &str, after: &str) -> Option<Range<usize>> {
    if before == after {
        return None;
    }
    let prefix = before
        .char_indices()
        .zip(after.chars())
        .find(|((_, a), b)| a != b)
        .map_or(before.len().min(after.len()), |((at, _), _)| at);
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();
    Some(prefix..after.len() - suffix)
}
