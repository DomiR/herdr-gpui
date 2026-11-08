//! `doc`: read and change Markdown documents through the app. Replies are
//! one message an agent can act on: what happened, the version now, and
//! what changed since the version it named.
use super::retry::{Backoff, is_busy};
use super::{Context, Output, unexpected};
use crate::{
    cli::control::{Change, DocCommand, FileAt, Text},
    control::{
        ErrorCode,
        protocol::{
            MAX_WAIT_SECONDS, Request, Response,
            doc::{DocOp, MAX_TEXT, Reply, TaskState},
            tabs::Reply as TabReply,
        },
    },
};
use std::{io::Read as _, path::Path, time::Duration};

mod link;

pub(super) fn run(command: DocCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        DocCommand::Open(FileAt { path, line }) => {
            let path = absolute(&path)?;
            let response = context.call(
                Request::Doc,
                DocOp::Open {
                    path: path.clone(),
                    line,
                },
            )?;
            return match response {
                Response::Tab {
                    reply: reply @ TabReply::Opened { .. },
                } => Output::data(format!("Opened {path}"), &reply),
                response => Err(unexpected(response)),
            };
        }
        DocCommand::Read { path, section } => DocOp::Read {
            path: absolute(&path)?,
            section,
        },
        DocCommand::Diff { path, since, wait } => DocOp::Diff {
            path: absolute(&path)?,
            since,
            wait,
            section: None,
            todos: false,
        },
        DocCommand::Watch {
            path,
            since,
            section,
            todos,
        } => {
            let watch = watch::Watch {
                shown: path.display().to_string(),
                path: absolute(&path)?,
                since,
                section,
                todos,
                json: context.json,
            };
            let mut stdout = std::io::stdout().lock();
            let status = watch::run(
                &watch,
                &mut |op| context.call(Request::Doc, op),
                &mut stdout,
                &mut std::thread::sleep,
            )?;
            return Ok(watch::ended(status));
        }
        DocCommand::Link { off, path } => DocOp::Link {
            path: absolute(&path)?,
            off,
        },
        DocCommand::Sections { path } => DocOp::Sections {
            path: absolute(&path)?,
        },
        DocCommand::TodoList {
            path,
            section,
            tags,
        } => DocOp::TodoList {
            path: absolute(&path)?,
            section,
            tags,
        },
        DocCommand::Append { path, change, text } => {
            let Change { version, section } = change;
            DocOp::Append {
                path: absolute(&path)?,
                version,
                section,
                text: read_text(text)?,
            }
        }
        DocCommand::Edit {
            path,
            change,
            old,
            new,
        } => DocOp::Edit {
            path: absolute(&path)?,
            version: change.version,
            section: change.section,
            old,
            new,
        },
        DocCommand::TodoAdd { path, change, text } => DocOp::TodoAdd {
            path: absolute(&path)?,
            version: change.version,
            section: change.section,
            text: read_text(text)?,
        },
        DocCommand::TodoCheck {
            path,
            change,
            todo,
            done,
        } => DocOp::TodoCheck {
            path: absolute(&path)?,
            version: change.version,
            section: change.section,
            todo,
            done,
        },
        DocCommand::TodoState {
            path,
            change,
            todo,
            state,
        } => DocOp::TodoState {
            path: absolute(&path)?,
            version: change.version,
            section: change.section,
            todo,
            state,
        },
    };
    let response = match op {
        DocOp::Diff {
            path,
            since,
            wait,
            section,
            todos,
        } => diff_waiting(
            wait,
            &mut |wait| {
                context.call(
                    Request::Doc,
                    DocOp::Diff {
                        path: path.clone(),
                        since,
                        wait,
                        section: section.clone(),
                        todos,
                    },
                )
            },
            &mut std::thread::sleep,
        )?,
        op => context.call(Request::Doc, op)?,
    };
    match response {
        Response::Doc { reply } => {
            let (text, failed) = message(&reply);
            let unchanged =
                matches!(&reply, Reply::Diff { diff: Some(diff), .. } if diff.is_empty());
            let output = Output::data(text, &reply)?;
            Ok(if failed {
                output.failed()
            } else if unchanged {
                // Nothing yet: exit 4, as `browser feedback` does.
                output.nothing()
            } else {
                output
            })
        }
        response => Err(unexpected(response)),
    }
}

/// `doc diff --wait`: asks again while Sheprd is busy, out of the same
/// wait, and never with less than a second of it left.
fn diff_waiting(
    wait: u64,
    ask: &mut dyn FnMut(u64) -> crate::Result<Response>,
    pause: &mut dyn FnMut(Duration),
) -> crate::Result<Response> {
    let mut left = Duration::from_secs(wait.min(MAX_WAIT_SECONDS));
    let mut backoff = Backoff::new();
    loop {
        match ask(left.as_secs()) {
            Ok(response) => return Ok(response),
            Err(error) if is_busy(&error) => {
                let delay = backoff.next().min(left);
                if left - delay < Duration::from_secs(1) {
                    return Err(error);
                }
                pause(delay);
                left -= delay;
            }
            Err(error) => return Err(error),
        }
    }
}

/// What a reply says, and whether it reports a change that did not apply.
pub(crate) fn message(reply: &Reply) -> (String, bool) {
    match reply {
        Reply::Text { version, text, .. } => (format!("Version: {version}\n\n{text}"), false),
        Reply::Diff {
            path,
            since,
            version,
            diff,
            ..
        } => match diff.as_deref() {
            Some("") => (
                format!("Version: {version}. No changes since {since}."),
                false,
            ),
            Some(diff) => (
                format!("Version: {version}. Changes since {since}:\n{diff}"),
                false,
            ),
            None => (
                format!(
                    "Version: {version}. Changes since {since} are not available; run sheprd doc read {path}."
                ),
                false,
            ),
        },
        Reply::Sections {
            version, sections, ..
        } => {
            let mut text = format!("Version: {version}");
            for section in sections {
                let indent = "  ".repeat(usize::from(section.level.saturating_sub(1)));
                text.push_str(&format!(
                    "\n{indent}{}  {} (line {})",
                    section.id, section.title, section.line
                ));
            }
            (text, false)
        }
        Reply::Todos { version, todos, .. } => {
            let mut text = format!("Version: {version}");
            for todo in todos {
                let indent = " ".repeat(todo.depth);
                let state = if todo.done {
                    TaskState::Done
                } else {
                    todo.state
                };
                let mark = match state {
                    TaskState::Open => " ",
                    TaskState::Done => "x",
                    TaskState::Cancelled => "-",
                };
                text.push_str(&format!(
                    "\n{indent}{}. [{mark}] {}",
                    todo.number, todo.text
                ));
            }
            if todos.is_empty() {
                text.push_str("\nNo todos.");
            }
            (text, false)
        }
        Reply::Updated {
            version,
            section,
            others,
            ..
        } => match (section, others) {
            (_, None) => (format!("Updated. Version: {version}"), false),
            (section, Some(others)) => {
                let name = section
                    .as_deref()
                    .map_or_else(String::new, |id| format!(" section \"{id}\""));
                let since = others.since;
                let changes = match &others.diff {
                    Some(diff) => format!(" Other sections changed since {since}:\n{diff}"),
                    None => format!(
                        " Other sections changed since {since}; run sheprd doc diff with --since {since} for them."
                    ),
                };
                (
                    format!("Updated{name}. Version: {version}.{changes}"),
                    false,
                )
            }
        },
        Reply::Mismatch {
            path,
            expected,
            version,
            diff,
        } => {
            let tail = match diff {
                Some(diff) => format!(" Changes since {expected}:\n{diff}"),
                None => format!(" Run sheprd doc read {path} for the current text."),
            };
            (
                format!("Version mismatch, nothing changed. Latest version: {version}.{tail}"),
                true,
            )
        }
        Reply::Link { status, .. } => (link::status_message(status), false),
    }
}

/// `path` as an absolute path, resolved here: the app has another current
/// directory. The file need not exist yet for the app to say so.
fn absolute(path: &Path) -> crate::Result<String> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|error| refused(format!("Cannot resolve {}: {error}", path.display())))?
            .join(path)
    };
    let absolute = absolute.canonicalize().unwrap_or(absolute);
    absolute
        .into_os_string()
        .into_string()
        .map_err(|_| refused("The path must be UTF-8".to_owned()))
}

/// The text given, or standard input, bounded.
fn read_text(text: Text) -> crate::Result<String> {
    match text {
        Text::Inline(text) => Ok(text),
        Text::Stdin => {
            let mut text = String::new();
            std::io::stdin()
                .take(MAX_TEXT as u64 + 1)
                .read_to_string(&mut text)
                .map_err(|error| refused(format!("Cannot read standard input: {error}")))?;
            if text.len() > MAX_TEXT {
                return Err(refused(format!("The text is over {MAX_TEXT} bytes")));
            }
            Ok(text)
        }
    }
}

fn refused(message: String) -> crate::Error {
    crate::control::Error::Rejected {
        code: ErrorCode::InvalidRequest,
        message,
    }
    .into()
}

mod watch;

#[cfg(test)]
mod tests;
