//! A document tab's view: the shared buffer it shows, and what is this
//! view's own: the selection, the composition, folds, the done filter,
//! scrolling, and the highlights of other writers' recent changes.
//!
//! Positions are byte offsets into the document's text. Between changes
//! the selection and folds are also kept as CRDT anchors, so a change made
//! elsewhere (another tab, an agent, the disk) moves them with the text.

use super::{
    Request,
    comments::{Comment, Draft},
    links::{self, FileLinks},
};
use super::{
    blocks::{self, Block},
    buffer::{Changed, DocBuffer, Who},
    layout::{DisplayMap, Look},
};
use crate::chat::visual::Visuals;
use crate::{browser::TabId, motion};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, ListAlignment, ListState, Pixels,
    Subscription, TextLayout, px,
};
use herdr_doc::{
    Anchor, Edit, Version,
    markdown::{self, Fold, Line},
    section, todo,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    time::{Duration, Instant},
};

/// How long another writer's change stays highlighted.
pub(super) const HIGHLIGHT: Duration = Duration::from_millis(4000);
/// Requests a view keeps for its window at most.
const MAX_REQUESTS: usize = 16;
/// Rows laid out beyond the visible ones.
const OVERDRAW: f32 = 400.;

/// The selection: `anchor` stays put while `head` (the caret) moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Selection {
    pub(super) anchor: usize,
    pub(super) head: usize,
}

impl Selection {
    pub(super) fn caret(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    pub(super) fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Another writer's recent change.
pub(super) struct Highlight {
    pub(super) start: Anchor,
    pub(super) end: Anchor,
    /// Who made it, as the editor names them; `None` for the disk.
    pub(super) by: Option<String>,
    pub(super) since: Instant,
}

/// The document's lines and what this view shows of them, prepared once
/// per version and fold change rather than on every frame.
#[derive(Default)]
pub(super) struct Prepared {
    pub(super) version: Option<Version>,
    pub(super) lines: Vec<Line>,
    pub(super) folds: Vec<Fold>,
    /// Done and total tasks under each heading, by its line.
    pub(super) counts: HashMap<usize, (usize, usize)>,
    /// The lines drawn, in order.
    pub(super) rows: Vec<usize>,
    /// Folded lines, and how many lines each hides.
    pub(super) folded: HashMap<usize, usize>,
    /// Code blocks, diagrams, and display math.
    pub(super) blocks: Vec<Block>,
    /// The picture block the caret is in, drawn as its source.
    pub(super) open_picture: Option<usize>,
}

impl Prepared {
    /// The row showing `line`, or the nearest shown line above it.
    pub(super) fn row_of(&self, line: usize) -> usize {
        self.rows
            .partition_point(|shown| *shown <= line)
            .saturating_sub(1)
    }

    pub(super) fn fold_at(&self, line: usize) -> Option<&Fold> {
        self.folds.iter().find(|fold| fold.line == line)
    }

    /// The picture block drawn on `line`, its opening line, while closed.
    pub(super) fn picture_at(&self, line: usize) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|block| block.open == line && block.is_picture())
            .filter(|block| self.open_picture != Some(block.open))
    }

    /// The picture block covering `line`.
    fn picture_around(&self, line: usize) -> Option<usize> {
        self.blocks
            .iter()
            .find(|block| block.is_picture() && block.lines().contains(&line))
            .map(|block| block.open)
    }
}

/// Each painted line's layout and drawn-text map, by line.
pub(super) type Layouts = Rc<RefCell<HashMap<usize, (TextLayout, Rc<DisplayMap>)>>>;

/// The section IDs folded, for the tab to save.
pub(crate) struct FoldsChanged(pub(crate) Vec<String>);

pub(crate) struct DocEditor {
    pub(crate) focus: FocusHandle,
    pub(super) buffer: Entity<DocBuffer>,
    pub(super) tab: TabId,
    pub(super) look: Look,
    pub(super) selection: Selection,
    /// The selection as anchors, as of the last change this view saw.
    anchors: Option<(Anchor, Anchor)>,
    /// Where vertical moves aim, kept across them.
    pub(super) goal_x: Option<Pixels>,
    /// The input method's composition, in the text.
    pub(super) marked: Option<Range<usize>>,
    /// The start of each folded line.
    folded: Vec<Anchor>,
    pub(super) hide_done: bool,
    pub(super) list: ListState,
    pub(super) prepared: Prepared,
    stale: bool,
    /// Each drawn line's text layout and how it maps to the source line,
    /// by line, as last painted.
    pub(super) layouts: Layouts,
    /// Code colours, diagrams, and formulas, drawn off the UI thread.
    pub(super) visuals: Visuals,
    /// Which code spans name files.
    pub(super) links: FileLinks,
    /// What this view asks of its window: files to open, notes to send.
    requests: Vec<Request>,
    /// Comments waiting to be sent, and the one being written.
    pub(super) comments: Vec<Comment>,
    pub(super) draft: Option<Draft>,
    pub(super) highlights: Vec<Highlight>,
    pub(super) selecting: bool,
    /// Folds and the line to show, waiting for the text to load.
    restore: Option<(Vec<String>, Option<u32>)>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FoldsChanged> for DocEditor {}

impl DocEditor {
    pub(crate) fn new(
        tab: TabId,
        buffer: Entity<DocBuffer>,
        look: Look,
        folded: Vec<String>,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) -> Self {
        let changed = cx.subscribe(&buffer, |this, _, event: &Changed, cx| {
            this.changed(event, cx);
        });
        let observed = cx.observe(&buffer, |this, _, cx| {
            this.stale = true;
            this.restore(cx);
            cx.notify();
        });
        let mut editor = Self {
            focus: cx.focus_handle(),
            buffer,
            tab,
            look,
            selection: Selection::default(),
            anchors: None,
            goal_x: None,
            marked: None,
            folded: Vec::new(),
            hide_done: false,
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            prepared: Prepared::default(),
            stale: true,
            layouts: Rc::default(),
            visuals: Visuals::default(),
            links: FileLinks::default(),
            requests: Vec::new(),
            comments: Vec::new(),
            draft: None,
            highlights: Vec::new(),
            selecting: false,
            restore: Some((folded, line)),
            _subscriptions: vec![changed, observed],
        };
        editor.restore(cx);
        editor
    }

    pub(crate) fn tab(&self) -> TabId {
        self.tab
    }

    /// Asks the window to act; it takes requests on its next tick.
    pub(super) fn request(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.requests.len() < MAX_REQUESTS {
            self.requests.push(request);
        }
        cx.notify();
    }

    /// What this view asked of its window since last taken.
    pub(crate) fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    pub(crate) fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        if self.look != look {
            self.look = look;
            self.list.remeasure();
            cx.notify();
        }
    }

    /// The document's text, empty while it loads.
    pub(super) fn text<'a>(&self, cx: &'a App) -> &'a str {
        self.buffer
            .read(cx)
            .document()
            .map_or("", herdr_doc::Document::text)
    }

    pub(super) fn version(&self, cx: &App) -> Option<Version> {
        self.buffer
            .read(cx)
            .document()
            .map(herdr_doc::Document::version)
    }

    /// Applies folds and the line a restored or newly opened tab asked for,
    /// once the text is there.
    fn restore(&mut self, cx: &mut Context<Self>) {
        if self.version(cx).is_none() {
            return;
        }
        let Some((ids, line)) = self.restore.take() else {
            return;
        };
        let text = self.text(cx).to_owned();
        for id in ids {
            if let Ok(found) = section::find(&text, &id) {
                self.fold_line(found.lines.start, &text, cx);
            }
        }
        self.stale = true;
        self.prepare(cx);
        if let Some(line) = line {
            self.go_to_line(line.saturating_sub(1) as usize, cx);
        }
    }

    /// Opens at 0-based `line`, unfolding what hides it.
    pub(crate) fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.prepare(cx);
        let Some(start) = self.prepared.lines.get(line).map(|l| l.range.start) else {
            return;
        };
        self.unfold_around(line, cx);
        self.select(Selection::caret(start), cx);
        self.reveal(cx);
    }

    /// Recomputes lines and rows when the text, folds, or filter changed.
    pub(super) fn prepare(&mut self, cx: &mut Context<Self>) {
        let version = self.version(cx);
        let open_picture = self.prepared.picture_around(self.caret_line());
        if !self.stale
            && self.prepared.version == version
            && self.prepared.open_picture == open_picture
        {
            return;
        }
        self.stale = false;
        let text = self.text(cx).to_owned();
        let text = text.as_str();
        let lines = markdown::lines(text);
        let folds = markdown::folds(&lines);
        let counts = todo::section_counts(&lines)
            .into_iter()
            .map(|(line, done, total)| (line, (done, total)))
            .collect();
        let starts: HashSet<usize> = {
            let doc = self.buffer.read(cx).document();
            self.folded
                .iter()
                .filter_map(|anchor| doc?.resolve(anchor))
                .map(|offset| markdown::line_at(&lines, offset))
                .collect()
        };
        let mut hidden = vec![false; lines.len()];
        if self.hide_done {
            hidden = todo::done_lines(&lines);
        }
        self.probe_links(links::candidates(text, &lines), cx);
        let found = blocks::blocks(text, &lines);
        let blocks = self.want_visuals(found);
        // A picture's source shows only while the caret is in it.
        let caret_line = markdown::line_at(&lines, self.selection.head);
        let open_picture = blocks
            .iter()
            .find(|block| block.is_picture() && block.lines().contains(&caret_line))
            .map(|block| block.open);
        for block in blocks.iter().filter(|block| block.is_picture()) {
            if open_picture != Some(block.open) {
                let lines = block.lines();
                for flag in &mut hidden[lines.start + 1..lines.end] {
                    *flag = true;
                }
            }
        }
        let mut folded = HashMap::new();
        for fold in &folds {
            if starts.contains(&fold.line) && !hidden[fold.line] {
                folded.insert(fold.line, fold.hidden.len());
                for flag in &mut hidden[fold.hidden.clone()] {
                    *flag = true;
                }
            }
        }
        let rows: Vec<usize> = (0..lines.len()).filter(|line| !hidden[*line]).collect();
        let old = std::mem::take(&mut self.prepared.rows);
        splice(&self.list, &old, &rows);
        self.prepared = Prepared {
            version,
            lines,
            folds,
            counts,
            rows,
            folded,
            blocks,
            open_picture,
        };
        self.keep_caret_shown(cx);
    }

    /// A caret inside hidden lines goes to the end of the line hiding them.
    fn keep_caret_shown(&mut self, cx: &mut Context<Self>) {
        let line = self.caret_line();
        let row = self.prepared.row_of(line);
        let Some(&shown) = self.prepared.rows.get(row) else {
            return;
        };
        if shown != line {
            let end = self.prepared.lines[shown].range.end;
            self.selection = Selection::caret(end);
            self.remember(cx);
        }
    }

    pub(super) fn caret_line(&self) -> usize {
        markdown::line_at(&self.prepared.lines, self.selection.head)
    }

    /// Takes the selection's anchors again, after this view moved it.
    pub(super) fn remember(&mut self, cx: &App) {
        let doc = self.buffer.read(cx).document();
        self.anchors = doc.and_then(|doc| {
            Some((
                doc.anchor(self.selection.anchor, false)?,
                doc.anchor(self.selection.head, false)?,
            ))
        });
    }

    pub(super) fn select(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let len = self.text(cx).len();
        self.selection = Selection {
            anchor: selection.anchor.min(len),
            head: selection.head.min(len),
        };
        self.remember(cx);
        cx.notify();
    }

    /// A change to the document, from this view or anywhere else.
    fn changed(&mut self, event: &Changed, cx: &mut Context<Self>) {
        let doc = self.buffer.read(cx).document();
        if let (Some(doc), Some((anchor, head))) = (doc, &self.anchors)
            && let (Some(anchor), Some(head)) = (doc.resolve(anchor), doc.resolve(head))
        {
            self.selection = Selection { anchor, head };
        }
        if event.by != Who::User {
            // A composition's place is bytes, which another writer's text
            // just moved; it ends rather than replacing the wrong text.
            self.marked = None;
        }
        if let Some(doc) = doc
            && event.by != Who::User
        {
            let text = doc.text();
            let by = match &event.by {
                Who::Agent(author) => Some(author.label.clone()),
                Who::Disk | Who::User => None,
            };
            let since = motion::now();
            for range in &event.inserted {
                // The end anchors the last character, at its first byte.
                let last = text
                    .get(..range.end)
                    .and_then(|before| before.chars().next_back())
                    .map_or(1, char::len_utf8);
                let (Some(start), Some(end)) = (
                    doc.anchor(range.start, false),
                    doc.anchor(range.end.saturating_sub(last), false),
                ) else {
                    continue;
                };
                self.highlights.push(Highlight {
                    start,
                    end,
                    by: by.clone(),
                    since,
                });
            }
        }
        self.stale = true;
        self.remember(cx);
        cx.notify();
    }

    /// Highlights still showing, as ranges of the text with their opacity
    /// and who made them. Drops the finished ones.
    pub(super) fn live_highlights(&mut self, cx: &App) -> Vec<(Range<usize>, f32, Option<String>)> {
        let now = motion::now();
        let length = motion::duration(HIGHLIGHT);
        self.highlights
            .retain(|highlight| now.saturating_duration_since(highlight.since) < length);
        let Some(doc) = self.buffer.read(cx).document() else {
            return Vec::new();
        };
        let text = doc.text();
        self.highlights
            .iter()
            .filter_map(|highlight| {
                let start = doc.resolve(&highlight.start)?;
                let last = doc.resolve(&highlight.end)?;
                let end = last + text[last..].chars().next().map_or(0, char::len_utf8);
                let k = motion::progress(highlight.since, now, length).unwrap_or(1.);
                (start < end).then(|| (start..end, 1. - k, highlight.by.clone()))
            })
            .collect()
    }

    /// Replaces the selection (or the composition) with `text`, as typing.
    pub(super) fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        let range = self.marked.take().unwrap_or_else(|| self.selection.range());
        let caret = range.start + text.len();
        self.apply(vec![Edit::new(range, text)], caret, cx);
    }

    /// Applies the user's `edits` and puts the caret at `caret` (an offset
    /// in the text after them).
    pub(super) fn apply(&mut self, edits: Vec<Edit>, caret: usize, cx: &mut Context<Self>) {
        self.apply_selecting(edits, Selection::caret(caret), cx);
    }

    pub(super) fn apply_selecting(
        &mut self,
        edits: Vec<Edit>,
        selection: Selection,
        cx: &mut Context<Self>,
    ) {
        if edits.is_empty() {
            return;
        }
        let result = self.buffer.update(cx, |buffer, cx| buffer.edit(&edits, cx));
        if let Err(error) = result {
            tracing::warn!(%error, "A document edit was refused");
            return;
        }
        self.goal_x = None;
        self.stale = true;
        self.select(selection, cx);
        self.prepare(cx);
        self.reveal(cx);
    }

    /// Scrolls the caret's row into view.
    pub(super) fn reveal(&mut self, cx: &mut Context<Self>) {
        self.prepare(cx);
        let row = self.prepared.row_of(self.caret_line());
        self.list.scroll_to_reveal_item(row);
        cx.notify();
    }

    /// Folds or unfolds `line`'s fold.
    pub(super) fn toggle_fold(&mut self, line: usize, cx: &mut Context<Self>) {
        self.prepare(cx);
        if self.prepared.fold_at(line).is_none() {
            return;
        }
        if self.prepared.folded.contains_key(&line) {
            self.unfold_line(line, cx);
        } else {
            let text = self.text(cx).to_owned();
            self.fold_line(line, &text, cx);
        }
        self.folds_changed(cx);
    }

    fn fold_line(&mut self, line: usize, text: &str, cx: &mut Context<Self>) {
        let lines = markdown::lines(text);
        let Some(start) = lines.get(line).map(|l| l.range.start) else {
            return;
        };
        if let Some(anchor) = self
            .buffer
            .read(cx)
            .document()
            .and_then(|doc| doc.anchor(start, false))
        {
            self.folded.push(anchor);
        }
        self.stale = true;
    }

    fn unfold_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let lines = &self.prepared.lines;
        let doc = self.buffer.read(cx).document();
        self.folded.retain(|anchor| {
            doc.and_then(|doc| doc.resolve(anchor))
                .is_none_or(|offset| markdown::line_at(lines, offset) != line)
        });
        self.stale = true;
    }

    /// Unfolds every fold hiding `line`.
    fn unfold_around(&mut self, line: usize, cx: &mut Context<Self>) {
        let hiding: Vec<usize> = self
            .prepared
            .folds
            .iter()
            .filter(|fold| fold.hidden.contains(&line))
            .map(|fold| fold.line)
            .filter(|start| self.prepared.folded.contains_key(start))
            .collect();
        if hiding.is_empty() {
            return;
        }
        for start in hiding {
            self.unfold_line(start, cx);
        }
        self.prepare(cx);
        self.folds_changed(cx);
    }

    /// The fold the caret is in or on, innermost first.
    pub(super) fn fold_at_caret(&mut self, fold: bool, cx: &mut Context<Self>) {
        self.prepare(cx);
        let line = self.caret_line();
        let target = self
            .prepared
            .folds
            .iter()
            .filter(|f| f.line == line || f.hidden.contains(&line))
            .filter(|f| self.prepared.folded.contains_key(&f.line) != fold)
            .min_by_key(|f| f.hidden.len())
            .map(|f| f.line);
        if let Some(line) = target {
            self.toggle_fold(line, cx);
        }
    }

    /// Folds or unfolds every heading.
    pub(super) fn fold_all(&mut self, fold: bool, cx: &mut Context<Self>) {
        self.prepare(cx);
        self.folded.clear();
        if fold {
            let text = self.text(cx).to_owned();
            let headings: Vec<usize> = self
                .prepared
                .folds
                .iter()
                .filter(|f| self.prepared.lines[f.line].is_heading())
                .map(|f| f.line)
                .collect();
            for line in headings {
                self.fold_line(line, &text, cx);
            }
        }
        self.stale = true;
        self.prepare(cx);
        self.folds_changed(cx);
    }

    /// Tells the tab which sections are folded, to save them.
    fn folds_changed(&mut self, cx: &mut Context<Self>) {
        self.prepare(cx);
        let text = self.text(cx);
        let ids: Vec<String> = section::sections(text)
            .into_iter()
            .filter(|s| self.prepared.folded.contains_key(&s.lines.start))
            .map(|s| s.id)
            .collect();
        cx.emit(FoldsChanged(ids));
        cx.notify();
    }

    pub(super) fn toggle_hide_done(&mut self, cx: &mut Context<Self>) {
        self.hide_done = !self.hide_done;
        self.stale = true;
        self.prepare(cx);
        cx.notify();
    }

    /// Checks or unchecks the task on `line`, keeping the selection.
    pub(super) fn toggle_task(&mut self, line: usize, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let Some(edit) = todo::toggle(text, line) else {
            return;
        };
        let selection = self.selection;
        self.apply_selecting(vec![edit], selection, cx);
    }

    pub(super) fn undo(&mut self, redo: bool, cx: &mut Context<Self>) {
        let changed = self
            .buffer
            .update(cx, |buffer, cx| buffer.history(redo, cx));
        match changed {
            Ok(Some(range)) => {
                self.stale = true;
                self.select(Selection::caret(range.end), cx);
                self.reveal(cx);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "Undo failed"),
        }
    }
}

/// Replaces the list's rows `old` with `new`, splicing only the part that
/// changed, so the scroll position and measured heights elsewhere hold.
fn splice(list: &ListState, old: &[usize], new: &[usize]) {
    if list.item_count() != old.len() {
        list.reset(new.len());
        return;
    }
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    if prefix == old_end && prefix == new_end {
        // Same rows, but a line's text may have changed: lay it out again.
        list.remeasure();
        return;
    }
    list.splice(prefix..old_end, new_end - prefix);
}

#[cfg(test)]
pub(super) fn editor_text(editor: &Entity<DocEditor>, cx: &App) -> String {
    editor.read(cx).text(cx).to_owned()
}
