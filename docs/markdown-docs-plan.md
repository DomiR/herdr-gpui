# Markdown documents: plan and decisions

The Markdown editor for todo lists: a document tab that edits any `.md` file
in place, and that the user and several agents can change at once without
lost edits. This file records the plan and the decisions made while
building it without the user watching, so a reviewer can check each one.

## Shape

| Layer | Where | What |
| --- | --- | --- |
| Document | `crates/herdr-doc` | The Loro CRDT text, disk merge, Markdown lines and inline styles, todo operations; no GPUI, all tested headless |
| Buffer | `crates/herdr-gpui/src/doc_view/buffer.rs` | One per open file for the whole app (a GPUI global keeps them), shared by every tab and window; owns the file's polling and writes |
| Editor | `crates/herdr-gpui/src/doc_view/` | The tab view: editing, IME, selection, undo, styling, todo interactions, agent highlights |
| Commands | `herdr-control-protocol` `doc` family, `control/server/doc.rs`, `control/client/doc.rs` | `doc open`, `doc read`, `doc append`, `doc todo add|check|list` |

## Decisions

1. **The file on disk is the source of truth.** The CRDT history lives only
   in memory, per open file. Reopening after a restart rebuilds it from the
   file. Nothing besides the Markdown is written.
2. **Merging an agent's file write.** The buffer remembers the text last
   synced with the disk and the CRDT version it was at. When the file
   changes, the new text is diffed against that synced text (`similar`: lines
   first, then characters inside each changed hunk, under a deadline) and
   applied as CRDT edits on a fork at the synced version, by a fresh peer.
   The fork's updates are imported into the live document, so they merge
   with what the user typed since. The merged text is then written back.
3. **Self-write suppression is by content.** A disk text equal to the synced
   text, or to the current text, is never applied as a change. One I/O task
   per buffer runs polls and writes in turn, so a poll never reads the
   buffer's own write while that write is still in flight.
4. **Write races.** Before writing, the buffer reads the file again; if it
   differs from the synced text, that change is merged first and the write
   tried again. The window between that read and the atomic rename remains;
   it is milliseconds wide and documented.
5. **Writes are atomic** (temp file in the same folder, then rename) and
   keep the file's permissions. Typing is written after 400 ms of quiet;
   agent commands are written before the command returns, so an agent that
   reads the file next sees its change.
6. **Watching is polling** (file length and modification time every 500 ms
   on the background executor), not a new `notify` dependency. It is cheap
   and catches editors that replace the file by rename.
7. **Deleted or moved files** stop syncing: the text stays in the tab, a
   banner says the file is gone, and nothing recreates it until the user
   saves with Cmd-S. Renames cannot be followed without a watcher.
8. **Refused files**: not UTF-8, larger than 2 MiB, or not a regular file,
   each with its own typed error.
9. **Agents through commands** edit through a fork at the latest version
   with that pane's own Loro peer, imported like a remote change. The undo
   manager tracks only the local peer, so undo never reverts an agent's
   edit. Each command's inserted text gets a highlight naming the pane.
10. **Agents through files** get a highlight labelled as a change on disk:
    the app cannot know which pane wrote the file.
11. **Which files open as documents**: every `.md` and `.markdown` file the
    app would open in a code tab (code search, file links, `tab code`) opens
    as a document instead, and `herdr-gpui doc open PATH[:LINE]` opens one
    directly.
12. **Styling keeps the source.** Markdown markers stay in the text and are
    drawn muted (or transparent where a checkbox replaces them), so every
    byte keeps its place and the caret, selection, and IME map one to one.
    Headings draw larger, code in the terminal font, links underlined.
13. **Long lines wrap**; the editor renders only the lines in view through
    GPUI's `list`, so a 5,000-line file stays smooth.
14. **History size**: the in-memory history grows with the session. It is
    not compacted yet.
15. **Folding** (asked for mid-run, for heavy use): a heading folds its
    section (to the next heading of the same or a higher level), a list
    item folds its sub-items, and a fenced code block folds to its first
    line. A chevron in the gutter toggles one; Cmd-Alt-[ and Cmd-Alt-]
    fold and unfold the one at the caret, and Cmd-Alt-Shift-[ and ] fold
    and unfold every heading. Folds are the view's, not the file's: they
    are saved with the tab by each heading's text, so a restored tab and an
    agent's edits above keep the same sections folded. Done/total counts
    stay visible on a folded heading. The caret never rests inside a fold:
    moving into one skips it, and an edit an agent makes inside it leaves
    it folded.
16. **Version numbers for agents' commands** (asked for mid-run): every
    change to a document, from any writer, takes the next version number,
    starting at 1 when it opens. `doc read` reports it. Every changing
    command (`doc append`, `doc edit`, `doc todo add`, `doc todo check`)
    must name the version it was made against (`--version N`). When the
    document has moved on, nothing is applied: the reply is a conflict
    carrying the current version and a unified diff from the named
    version's text to the current one, so the agent sees exactly what
    changed and sends a new command against the new number. The document
    keeps the CRDT version of its last 512 numbers to rebuild an old text
    for that diff; an older number gets the conflict without a diff,
    telling the agent to read the document again, and so does a diff over
    64 KiB. A successful change replies with its new number. The user's
    typing changes the number too, so an agent racing the user is told,
    not silently merged. The app remembers each file's last number and
    text for the session, so a document closed and opened again carries
    on counting (one more when the file changed meanwhile). Numbers start
    again at 1 after the app restarts: a stale number from before a
    restart is refused only if the text differs from what that number
    named then, which the app no longer knows; this is the one gap.
17. **`doc edit`**: replaces one exact occurrence of `--old` with `--new`
    (the occurrence must be unique), the precise edit agents already know
    from their file tools, but without the read-modify-write race.
18. **Sections** (asked for mid-run): each `#`, `##`, or `###` heading
    starts a section that runs to the next such heading; deeper headings
    stay inside it, and text before the first heading is in no section. A
    section's ID is its heading's slug as GitHub (and most Markdown
    renderers) make anchor IDs: lowercased, punctuation dropped, spaces as
    hyphens, a repeated slug suffixed `-1`, `-2`. Changing commands take
    `--section ID`, which both scopes them (a todo is added, found, or an
    `edit` searched only inside it; `append` lands at its end) and relaxes
    the version check: a stale version is accepted when that section's
    text is the same now as at the version named, so two agents working on
    different sections never block each other. The reply then carries the
    diff of what changed elsewhere since that version.
19. **Replies are one LLM-readable message**, as the user worded them:
    `Updated. Version: 7`; `Updated section "goals". Version: 7. Other
    sections changed since 5:` and the diff; `Version mismatch, nothing
    changed. Latest version: 7. Changes since 5:` and the diff, or, without
    one, `Run herdr-gpui doc read PATH for the current text.` `--json`
    carries the same as fields.
20. **An agent always knows the document's state** (the goal the user
    named): `doc read` gives the text and its version; a successful change
    against the current version leaves the document at exactly that text
    plus the change, under the number the reply names; every other reply
    (a section change over a stale version, a mismatch) carries the diff
    from the version the agent had; and `doc diff PATH --since N` gives the
    changes since N on demand, without reading the whole text again.
21. **Docket**: `/dev` would plan this through docket, but this repository
    has no docket binding and bootstrapping one needs operator approvals
    (trust entries and activation) the unattended run cannot give. The
    phases ran directly, each committed on its own.
