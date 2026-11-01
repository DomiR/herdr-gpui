//! One open Markdown file for the whole app: its CRDT document, shared by
//! every tab and window that shows it and by agents' `doc` commands, and the
//! single I/O pump that reads and writes the file.
//!
//! The pump runs one disk job at a time, polls or writes, so a poll never
//! reads the buffer's own write while it is in flight. Typing is written
//! after a short quiet spell; a disk change is merged and the merged text
//! written back. A file that disappears stops syncing until the user saves.

use super::Error;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Global, Task, WeakEntity};
use herdr_doc::{
    Applied, Author, Document, Edit, Merge, Scope, Synced, Version, diff,
    disk::{self, Stamp, Written},
};
use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// How often the file is checked for changes.
const POLL: Duration = Duration::from_millis(500);
/// How often a closing document merges and writes again before it gives up.
const CLOSING_WRITES: usize = 4;
/// How long typing pauses before it is written.
const QUIET: Duration = Duration::from_millis(400);

/// Who changed a document, when it was not the person in this app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Who {
    /// The person, in this or another tab.
    User,
    /// An agent's `doc` command.
    Agent(Author),
    /// Another program rewrote the file.
    Disk,
}

/// A change to the document, for every editor showing it.
#[derive(Clone, Debug)]
pub(crate) struct Changed {
    pub(crate) by: Who,
    /// What it inserted, in the new text.
    pub(crate) inserted: Vec<Range<usize>>,
}

/// Where the file stands.
#[derive(Debug, Default)]
pub(crate) enum State {
    #[default]
    Loading,
    Ready,
    /// The file could not be read at all.
    Failed(Error),
}

pub(crate) struct DocBuffer {
    path: PathBuf,
    doc: Option<Document>,
    state: State,
    /// The file is gone: nothing is read or written until the user saves.
    missing: bool,
    /// A disk job is running.
    busy: bool,
    stamp: Option<Stamp>,
    /// When the user last typed, which a write waits out.
    typed: Option<Instant>,
    /// Write as soon as the pump is free, without waiting for quiet.
    write_now: bool,
    /// Write even though the file is gone, as Cmd-S asks.
    recreate: bool,
    /// The poll timer fired: read the file's stamp when next free.
    poll_due: bool,
    /// Why the last write failed, while that holds.
    write_error: Option<herdr_doc::Error>,
    _poll: Task<()>,
    debounce: Option<Task<()>>,
}

/// A changed file read by a poll, with its diff from the synced text.
struct Read {
    text: String,
    stamp: Stamp,
    base: String,
    edits: Vec<Edit>,
}

/// Every open document, by path, and the last version of each that closed,
/// so a file opened again in the same session carries on counting.
#[derive(Default)]
pub(crate) struct Docs {
    open: HashMap<PathBuf, WeakEntity<DocBuffer>>,
    closed: HashMap<PathBuf, (Version, u64)>,
}

impl Global for Docs {}

impl Docs {
    /// The buffer for `path`, made and loading when none is open.
    pub(crate) fn open(path: &Path, cx: &mut App) -> Entity<DocBuffer> {
        if let Some(buffer) = cx
            .default_global::<Docs>()
            .open
            .get(path)
            .and_then(WeakEntity::upgrade)
        {
            return buffer;
        }
        let buffer = cx.new(|cx| DocBuffer::new(path.to_owned(), cx));
        let docs = cx.global_mut::<Docs>();
        docs.open.retain(|_, weak| weak.upgrade().is_some());
        docs.open.insert(path.to_owned(), buffer.downgrade());
        buffer
    }
}

fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

impl EventEmitter<Changed> for DocBuffer {}

impl DocBuffer {
    fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let Ok(()) = this.update(cx, |this, cx| {
                    this.poll_due = true;
                    this.pump(cx);
                }) else {
                    break;
                };
            }
        });
        cx.on_release(|this, cx| this.release(cx)).detach();
        let mut buffer = Self {
            path,
            doc: None,
            state: State::Loading,
            missing: false,
            busy: false,
            stamp: None,
            typed: None,
            write_now: false,
            recreate: false,
            poll_due: false,
            write_error: None,
            _poll: poll,
            debounce: None,
        };
        buffer.load(cx);
        buffer
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn document(&self) -> Option<&Document> {
        self.doc.as_ref()
    }

    pub(crate) fn state(&self) -> &State {
        &self.state
    }

    pub(crate) fn missing(&self) -> bool {
        self.missing
    }

    pub(crate) fn write_error(&self) -> Option<&herdr_doc::Error> {
        self.write_error.as_ref()
    }

    /// The newest version the file holds every change of.
    pub(crate) fn written(&self) -> Option<Version> {
        self.doc.as_ref().map(|doc| doc.synced().version())
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        let path = self.path.clone();
        let read = cx
            .background_executor()
            .spawn(async move { disk::read(&path) });
        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((text, stamp)) => {
                        let version = this.reopened_version(&text, cx);
                        match Document::at(&text, version) {
                            Ok(doc) => {
                                this.doc = Some(doc);
                                this.stamp = Some(stamp);
                                this.state = State::Ready;
                            }
                            Err(error) => this.state = State::Failed(error.into()),
                        }
                    }
                    Err(error) => this.state = State::Failed(error.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The version a file opened again starts at: where it left off, one
    /// more when it changed meanwhile.
    fn reopened_version(&self, text: &str, cx: &App) -> Version {
        let closed = cx
            .try_global::<Docs>()
            .and_then(|docs| docs.closed.get(&self.path));
        match closed {
            Some((version, hash)) if *hash == text_hash(text) => *version,
            Some((version, _)) => version.next(),
            None => Version::FIRST,
        }
    }

    /// Writes what is unwritten once the last editor and command let go,
    /// and remembers the version for the next open.
    fn release(&mut self, cx: &mut App) {
        let Some(mut doc) = self.doc.take() else {
            return;
        };
        cx.default_global::<Docs>()
            .closed
            .insert(self.path.clone(), (doc.version(), text_hash(doc.text())));
        if !doc.dirty() || self.missing {
            return;
        }
        // The document goes with the write: a change on disk it has not
        // seen, or a write of its own still landing, is merged and the
        // write tried again, rather than either losing the last typing.
        let path = self.path.clone();
        cx.background_executor()
            .spawn(async move {
                for _ in 0..CLOSING_WRITES {
                    let write = doc.to_write();
                    let expected = doc.synced().text().to_owned();
                    match disk::write_if_unchanged(&path, &expected, write.text()) {
                        Ok(Written::Changed(disk)) => {
                            doc.not_written(&write);
                            if let Err(error) = doc.merge_disk(&disk) {
                                tracing::warn!(%error, "Could not merge a closed document");
                                return;
                            }
                            if !doc.dirty() {
                                return;
                            }
                        }
                        Ok(Written::Wrote(_) | Written::Missing) => return,
                        Err(error) => {
                            tracing::warn!(%error, "Could not write a closed document");
                            return;
                        }
                    }
                }
            })
            .detach();
    }

    /// The user's edits, as one undoable change.
    pub(crate) fn edit(&mut self, edits: &[Edit], cx: &mut Context<Self>) -> Result<(), Error> {
        let doc = self.doc.as_mut().ok_or(Error::Loading)?;
        doc.edit(edits)?;
        self.typed(cx);
        let inserted = changed_ranges(edits);
        cx.emit(Changed {
            by: Who::User,
            inserted,
        });
        cx.notify();
        Ok(())
    }

    /// Undoes (or redoes) the user's last step; the range it changed.
    pub(crate) fn history(
        &mut self,
        redo: bool,
        cx: &mut Context<Self>,
    ) -> Result<Option<Range<usize>>, Error> {
        let doc = self.doc.as_mut().ok_or(Error::Loading)?;
        let changed = if redo { doc.redo()? } else { doc.undo()? };
        if changed.is_some() {
            self.typed(cx);
            cx.emit(Changed {
                by: Who::User,
                inserted: Vec::new(),
            });
            cx.notify();
        }
        Ok(changed)
    }

    pub(crate) fn break_undo(&mut self) {
        if let Some(doc) = self.doc.as_mut() {
            doc.break_undo();
        }
    }

    /// An agent's change through a command; written at once.
    pub(crate) fn agent_edit(
        &mut self,
        author: &Author,
        scope: Scope<'_>,
        make: impl FnOnce(&str) -> herdr_doc::Result<Vec<Edit>>,
        cx: &mut Context<Self>,
    ) -> Result<Applied, Error> {
        if self.missing {
            return Err(Error::Missing);
        }
        let doc = self.doc.as_mut().ok_or(Error::Loading)?;
        let applied = doc.agent_edit(author, scope, make)?;
        if !applied.inserted.is_empty() || doc.dirty() {
            cx.emit(Changed {
                by: Who::Agent(author.clone()),
                inserted: applied.inserted.clone(),
            });
            self.write_now = true;
            self.pump(cx);
            cx.notify();
        }
        Ok(applied)
    }

    /// Cmd-S: writes now, and writes a file that is gone again.
    pub(crate) fn save(&mut self, cx: &mut Context<Self>) {
        if self.missing {
            self.recreate = true;
        }
        self.write_now = true;
        self.pump(cx);
    }

    /// Notes typing, and writes once it pauses.
    fn typed(&mut self, cx: &mut Context<Self>) {
        self.typed = Some(cx.background_executor().now());
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(QUIET).await;
            let _ = this.update(cx, |this, cx| this.pump(cx));
        }));
    }

    /// Starts the next disk job, if one is due and none is running: a write
    /// when the text is unwritten and typing paused (or a write was asked
    /// for), else a poll when the timer asked for one. Each job ends by
    /// calling this again, so a write that found a change merges and
    /// writes again, but polls never run back to back.
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.busy || !matches!(self.state, State::Ready) {
            return;
        }
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let now = cx.background_executor().now();
        let quiet = self
            .typed
            .is_none_or(|typed| now.saturating_duration_since(typed) >= QUIET);
        if doc.dirty() && (self.write_now || quiet) && (!self.missing || self.recreate) {
            self.write_now = false;
            let write = doc.to_write();
            let expected = doc.synced().text().to_owned();
            let recreate = std::mem::take(&mut self.recreate);
            self.start_write(write, expected, recreate, cx);
        } else if self.poll_due {
            self.poll_due = false;
            self.start_poll(cx);
        }
    }

    fn start_write(
        &mut self,
        write: Synced,
        expected: String,
        recreate: bool,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        let path = self.path.clone();
        let text = write.text().to_owned();
        let job = cx.background_executor().spawn(async move {
            if recreate {
                return disk::recreate(&path, &expected, &text);
            }
            disk::write_if_unchanged(&path, &expected, &text)
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.wrote(write, result, cx);
                this.pump(cx);
            });
        })
        .detach();
    }

    fn wrote(&mut self, write: Synced, result: herdr_doc::Result<Written>, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        match result {
            Ok(Written::Wrote(stamp)) => {
                doc.wrote(write);
                self.stamp = Some(stamp);
                self.missing = false;
                self.write_error = None;
            }
            Ok(Written::Changed(disk)) => {
                doc.not_written(&write);
                // Merge what another writer put there, then write again; a
                // merge that fails is reported rather than retried at once.
                self.write_now = self.merge(&disk, None, cx);
            }
            Ok(Written::Missing) => {
                doc.not_written(&write);
                self.missing = true;
            }
            Err(error) => {
                doc.not_written(&write);
                tracing::warn!(%error, path = %self.path.display(), "Could not write a document");
                self.write_error = Some(error);
            }
        }
        cx.notify();
    }

    fn start_poll(&mut self, cx: &mut Context<Self>) {
        self.busy = true;
        let path = self.path.clone();
        let known = self.stamp;
        let base = self
            .doc
            .as_ref()
            .map(|doc| doc.synced().text().to_owned())
            .unwrap_or_default();
        // The diff of a rewritten file is worked out here, off the UI thread.
        let job = cx.background_executor().spawn(async move {
            match disk::stamp(&path)? {
                None => Ok(None),
                Some(stamp) if Some(stamp) == known => Ok(Some(None)),
                Some(_) => disk::read(&path).map(|(text, stamp)| {
                    let edits = diff::edits(&base, &text);
                    Some(Some(Read {
                        text,
                        stamp,
                        base,
                        edits,
                    }))
                }),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(None) => {
                        this.missing = true;
                        cx.notify();
                    }
                    Ok(Some(None)) => {}
                    Ok(Some(Some(read))) => {
                        this.stamp = Some(read.stamp);
                        this.merge(&read.text, Some((&read.base, read.edits)), cx);
                    }
                    Err(herdr_doc::Error::Missing { .. }) => {
                        this.missing = true;
                        cx.notify();
                    }
                    Err(error) => {
                        tracing::warn!(%error, "Could not read a document's file");
                    }
                }
                this.pump(cx);
            });
        })
        .detach();
    }

    /// Merges a disk text read back, with its diff when worked out
    /// already; whether it merged.
    fn merge(
        &mut self,
        disk: &str,
        diffed: Option<(&str, Vec<Edit>)>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.doc.as_mut() else {
            return false;
        };
        self.missing = false;
        match doc.merge_disk_diffed(disk, diffed) {
            Ok(Merge::Unchanged) => true,
            Ok(Merge::Merged { inserted }) => {
                cx.emit(Changed {
                    by: Who::Disk,
                    inserted,
                });
                cx.notify();
                true
            }
            Err(error) => {
                tracing::warn!(%error, "Could not merge a document's file");
                self.write_error = Some(error);
                cx.notify();
                false
            }
        }
    }
}

/// The ranges `edits` (ascending once sorted) insert, in the new text.
fn changed_ranges(edits: &[Edit]) -> Vec<Range<usize>> {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    sorted.sort_by_key(|edit| edit.range.start);
    let mut shift: isize = 0;
    let mut out = Vec::new();
    for edit in sorted {
        let start = edit.range.start.saturating_add_signed(shift);
        if !edit.text.is_empty() {
            out.push(start..start + edit.text.len());
        }
        shift += edit.text.len() as isize - edit.range.len() as isize;
    }
    out
}
