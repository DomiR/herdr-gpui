//! `doc`: Markdown documents agents read and change through the app, so
//! their changes merge with the user's typing instead of racing it. A change
//! names the version it was made against; the reply says what happened in
//! words an agent can act on (`docs/markdown-docs-plan.md`).

use super::{Watches, socket, tabs::arrive};
use crate::{
    control::protocol::{
        Call, Caller, ErrorCode, Response,
        doc::{DocOp, Others, Reply, SectionInfo, TodoChangeInfo, TodoChangeKind, TodoInfo},
    },
    doc_view::{self, DocBuffer, Docs, State},
};
use gpui::{App, Entity};
use herdr_doc::{Author, Edit, Origin, Scope, Version, section, todo};
use std::{path::PathBuf, time::Duration};

/// Makes a change's edits from the text it applies to.
type Make = Box<dyn FnOnce(&str) -> herdr_doc::Result<Vec<Edit>>>;

/// The longest label a pane gets in a highlight.
const MAX_LABEL: usize = 60;

pub(super) fn serve(
    call: Call<DocOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Call { caller, op } = call;
    if !op.bounded() {
        return incoming.respond(invalid("A path or text is too long"));
    }
    if let DocOp::Open { path, line } = &op {
        let path = path.clone();
        return arrive::doc(&caller, &path, *line, incoming, watches, cx);
    }
    let path = match document_path(op.path()) {
        Ok(path) => path,
        Err(response) => return incoming.respond(response),
    };
    let author = author(&caller, cx);
    let buffer = Docs::open(&path, cx);
    let display = op.path().to_owned();
    let mut stage = Stage::Loading;
    let expired = match &op {
        // Waited for a change that did not come.
        DocOp::Diff { since, wait, .. } if *wait > 0 => Response::Doc {
            reply: Reply::Diff {
                path: display.clone(),
                since: *since,
                version: *since,
                diff: Some(String::new()),
                by: Vec::new(),
                todos: Vec::new(),
            },
        },
        _ => Response::error(
            ErrorCode::Timeout,
            format!("{display} was not read and written in time; try again"),
        ),
    };
    watches.add(
        incoming,
        Duration::from_secs(op.wait_seconds()),
        move |cx| step(&mut stage, &op, &buffer, &author, &display, cx),
        expired,
        cx,
    );
}

/// Where a change stands while it waits.
enum Stage {
    Loading,
    /// Applied as `version`; waiting for the file to hold it.
    Writing {
        version: Version,
        reply: Reply,
    },
}

/// Moves a request on; `Some` once it has its answer.
fn step(
    stage: &mut Stage,
    op: &DocOp,
    buffer: &Entity<DocBuffer>,
    author: &Author,
    path: &str,
    cx: &mut App,
) -> Option<Response> {
    match stage {
        Stage::Loading => {
            match buffer.read(cx).state() {
                State::Loading => return None,
                State::Failed(doc_view::Error::Doc(herdr_doc::Error::Missing { .. })) => {
                    return Some(gone(path));
                }
                State::Failed(error) => return Some(failed(error.to_string())),
                State::Ready => {}
            }
            // A watched document that disappears ends the watch.
            if matches!(op, DocOp::Diff { .. }) && buffer.read(cx).missing() {
                return Some(gone(path));
            }
            match apply(op, buffer, author, path, cx) {
                Ok(Applied::Answer(response)) => Some(response),
                Ok(Applied::Waiting) => None,
                Ok(Applied::Wrote { version, reply }) => {
                    *stage = Stage::Writing { version, reply };
                    None
                }
                Err(response) => Some(response),
            }
        }
        Stage::Writing { version, reply } => {
            let buffer = buffer.read(cx);
            // Written, or nothing left to write (the change undid itself,
            // or changed nothing).
            let clean = buffer.document().is_some_and(|doc| !doc.dirty());
            if clean || buffer.written().is_some_and(|written| written >= *version) {
                return Some(Response::Doc {
                    reply: reply.clone(),
                });
            }
            if let Some(error) = buffer.write_error() {
                return Some(failed(format!(
                    "The change is in the open document but could not be written: {error}"
                )));
            }
            if buffer.missing() {
                return Some(failed(format!(
                    "{path} was deleted or moved; the change is in the open document but not on disk"
                )));
            }
            None
        }
    }
}

enum Applied {
    /// Answered at once: a read, or a refused change.
    Answer(Response),
    /// Nothing to answer yet: `doc diff --wait` waits for a change.
    Waiting,
    /// Changed; answered once written.
    Wrote { version: Version, reply: Reply },
}

fn apply(
    op: &DocOp,
    buffer: &Entity<DocBuffer>,
    author: &Author,
    path: &str,
    cx: &mut App,
) -> Result<Applied, Response> {
    let doc = buffer
        .read(cx)
        .document()
        .ok_or_else(|| failed("The document did not load".to_owned()))?;
    let version = doc.version().0;
    let text = doc.text();
    let path = path.to_owned();
    let answer = |reply| Ok(Applied::Answer(Response::Doc { reply }));
    let (expected, section, make): (u64, Option<&String>, Make) = match op {
        DocOp::Open { .. } => return Err(invalid("Open is not a change")),
        DocOp::Read { section, .. } => {
            let text = match section {
                Some(id) => {
                    let found = section::find(text, id).map_err(refused)?;
                    text[found.range].to_owned()
                }
                None => text.to_owned(),
            };
            return answer(Reply::Text {
                path,
                version,
                section: section.clone(),
                text,
            });
        }
        DocOp::Diff {
            since,
            wait,
            section,
            todos,
            ..
        } => return diff(doc, path, *since, *wait, section.as_deref(), *todos),
        DocOp::Sections { .. } => {
            let sections = section::sections(text)
                .into_iter()
                .map(|found| SectionInfo {
                    id: found.id,
                    title: found.title,
                    level: found.level,
                    line: found.lines.start + 1,
                })
                .collect();
            return answer(Reply::Sections {
                path,
                version,
                sections,
            });
        }
        DocOp::TodoList { section, .. } => {
            let lines = match section {
                Some(id) => Some(section::find(text, id).map_err(refused)?.lines),
                None => None,
            };
            let todos = todo::todos(text)
                .into_iter()
                .filter(|t| lines.as_ref().is_none_or(|lines| lines.contains(&t.line)))
                .map(|t| TodoInfo {
                    number: t.number,
                    line: t.line + 1,
                    done: t.done,
                    text: t.text,
                    depth: t.depth,
                    section: t.section,
                })
                .collect();
            return answer(Reply::Todos {
                path,
                version,
                todos,
            });
        }
        DocOp::Append {
            version,
            section,
            text,
            ..
        } => {
            let addition = text.clone();
            let id = section.clone();
            (
                *version,
                section.as_ref(),
                Box::new(move |doc: &str| {
                    todo::append(doc, &addition, id.as_deref()).map(|edit| vec![edit])
                }),
            )
        }
        DocOp::Edit {
            version,
            section,
            old,
            new,
            ..
        } => {
            let (old, new, id) = (old.clone(), new.clone(), section.clone());
            (
                *version,
                section.as_ref(),
                Box::new(move |doc: &str| {
                    todo::replace(doc, &old, &new, id.as_deref()).map(|edit| vec![edit])
                }),
            )
        }
        DocOp::TodoAdd {
            version,
            section,
            text,
            ..
        } => {
            let (item, id) = (text.clone(), section.clone());
            (
                *version,
                section.as_ref(),
                Box::new(move |doc: &str| {
                    todo::add(doc, &item, id.as_deref()).map(|edit| vec![edit])
                }),
            )
        }
        DocOp::TodoCheck {
            version,
            section,
            todo: query,
            done,
            ..
        } => {
            let (query, id, done) = (query.clone(), section.clone(), *done);
            (
                *version,
                section.as_ref(),
                Box::new(move |doc: &str| {
                    let found = todo::find_in(doc, &query, id.as_deref())?;
                    Ok(todo::set_done(doc, found.line, done).into_iter().collect())
                }),
            )
        }
    };
    let scope = Scope {
        version: Version(expected),
        section: section.map(String::as_str),
    };
    let result = buffer.update(cx, |buffer, cx| buffer.agent_edit(author, scope, make, cx));
    match result {
        Ok(applied) => {
            let reply = Reply::Updated {
                path,
                version: applied.version.0,
                section: section.cloned(),
                others: applied.others.map(|(since, diff)| Others {
                    since: since.0,
                    diff,
                }),
            };
            Ok(Applied::Wrote {
                version: applied.version,
                reply,
            })
        }
        Err(doc_view::Error::Doc(herdr_doc::Error::Conflict(conflict))) => {
            answer(Reply::Mismatch {
                path,
                expected: conflict.expected.0,
                version: conflict.current.0,
                diff: conflict.diff,
            })
        }
        Err(error) => Err(refused(error)),
    }
}

/// `doc diff`: what changed since `since`, or [`Applied::Waiting`] while a
/// waiting request has seen no change it counts (in its section, or to
/// tasks, when it names them).
fn diff(
    doc: &herdr_doc::Document,
    path: String,
    since: u64,
    wait: u64,
    section: Option<&str>,
    todos: bool,
) -> Result<Applied, Response> {
    let version = doc.version().0;
    let Some(changes) = doc.changes(Version(since), section) else {
        // Too old, or a number from before the app restarted.
        return Ok(Applied::Answer(Response::Doc {
            reply: Reply::Diff {
                path,
                since,
                version,
                diff: None,
                by: Vec::new(),
                todos: Vec::new(),
            },
        }));
    };
    let changed = changes.diff.as_deref() != Some("");
    let counts = changed && (!todos || !changes.todos.is_empty());
    if wait > 0 && !counts {
        return Ok(Applied::Waiting);
    }
    Ok(Applied::Answer(Response::Doc {
        reply: Reply::Diff {
            path,
            since,
            version,
            diff: if counts {
                changes.diff
            } else {
                Some(String::new())
            },
            by: changes.by.iter().map(label).collect(),
            todos: if todos {
                changes
                    .todos
                    .into_iter()
                    .map(|change| TodoChangeInfo {
                        kind: kind(change.kind),
                        number: change.number,
                        text: change.text,
                    })
                    .collect()
            } else {
                Vec::new()
            },
        },
    }))
}

fn kind(kind: todo::ChangeKind) -> TodoChangeKind {
    match kind {
        todo::ChangeKind::Added => TodoChangeKind::Added,
        todo::ChangeKind::Removed => TodoChangeKind::Removed,
        todo::ChangeKind::Checked => TodoChangeKind::Checked,
        todo::ChangeKind::Unchecked => TodoChangeKind::Unchecked,
        todo::ChangeKind::Edited => TodoChangeKind::Edited,
    }
}

/// Who made a change, as `doc watch` names them. Agent-facing English.
fn label(origin: &Origin) -> String {
    match origin {
        Origin::User => "the user".to_owned(), // i18n: data
        Origin::Disk => "file".to_owned(),     // i18n: data
        Origin::Agent(author) if author.pane.is_empty() || author.label == author.pane => {
            author.label.clone()
        }
        Origin::Agent(author) => format!("{} ({})", author.label, author.pane),
    }
}

/// The answer once a watched or read document is gone.
fn gone(path: &str) -> Response {
    Response::error(
        ErrorCode::NotFound,
        format!("{path} does not exist; it may have been deleted or moved"),
    )
}

/// The document's file: an absolute Markdown path.
fn document_path(path: &str) -> Result<PathBuf, Response> {
    let path_ok = !path.is_empty()
        && !path.chars().any(char::is_control)
        && std::path::Path::new(path).is_absolute();
    if !path_ok {
        return Err(invalid(
            "The file must be an absolute path without control characters",
        ));
    }
    let path = PathBuf::from(path);
    if !doc_view::is_markdown(&path) {
        return Err(invalid("Only Markdown files (.md) are documents"));
    }
    Ok(path)
}

/// Who the caller is, as the editor's highlight names them: the agent in
/// the caller's pane and the pane's name, from a window showing it.
fn author(caller: &Caller, cx: &App) -> Author {
    let pane = caller.pane_id.clone().unwrap_or_default();
    let label = crate::control::main_windows(cx)
        .iter()
        .find_map(|handle| {
            handle
                .read_with(cx, |view, _| {
                    let snapshot = view.live.snapshot.as_ref()?;
                    let agent = snapshot.agents.iter().find(|agent| agent.pane_id == pane);
                    let named = snapshot
                        .panes
                        .iter()
                        .find(|candidate| candidate.pane_id == pane)
                        .and_then(|candidate| candidate.label.clone());
                    let who = agent.and_then(|agent| {
                        agent
                            .display_agent
                            .clone()
                            .or_else(|| agent.agent.clone())
                            .or_else(|| agent.name.clone())
                    });
                    match (who, named) {
                        (Some(who), Some(named)) => Some(format!("{who} · {named}")), // i18n: data
                        (Some(who), None) => Some(who),
                        (None, named) => named,
                    }
                })
                .ok()
                .flatten()
        })
        .unwrap_or_else(|| {
            if pane.is_empty() {
                "agent".to_owned()
            } else {
                pane.clone()
            }
        }); // i18n: data
    let label = crate::notifications::safe_text(&label, MAX_LABEL)
        .trim()
        .to_owned();
    Author { pane, label }
}

fn invalid(message: impl Into<String>) -> Response {
    Response::error(ErrorCode::InvalidRequest, message)
}

fn failed(message: String) -> Response {
    Response::error(ErrorCode::Failed, message)
}

fn refused(error: impl std::fmt::Display) -> Response {
    Response::error(ErrorCode::InvalidRequest, error.to_string())
}

#[cfg(test)]
mod tests;
