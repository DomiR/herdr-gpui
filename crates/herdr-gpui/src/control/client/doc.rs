//! `doc`: read and change Markdown documents through the app. Replies are
//! one message an agent can act on: what happened, the version now, and
//! what changed since the version it named.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::{Change, DocCommand, FileAt, Text},
    control::{
        ErrorCode,
        protocol::{
            Request, Response,
            doc::{DocOp, MAX_TEXT, Reply},
            tabs::Reply as TabReply,
        },
    },
};
use std::{io::Read as _, path::Path};

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
        },
        DocCommand::Sections { path } => DocOp::Sections {
            path: absolute(&path)?,
        },
        DocCommand::TodoList { path, section } => DocOp::TodoList {
            path: absolute(&path)?,
            section,
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
    };
    match context.call(Request::Doc, op)? {
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

/// What a reply says, and whether it reports a change that did not apply.
pub(crate) fn message(reply: &Reply) -> (String, bool) {
    match reply {
        Reply::Text { version, text, .. } => (format!("Version: {version}\n\n{text}"), false),
        Reply::Diff {
            path,
            since,
            version,
            diff,
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
                    "Version: {version}. Changes since {since} are not available; run herdr-gpui doc read {path}."
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
                let mark = if todo.done { "x" } else { " " };
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
                        " Other sections changed since {since}; run herdr-gpui doc diff with --since {since} for them."
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
                None => format!(" Run herdr-gpui doc read {path} for the current text."),
            };
            (
                format!("Version mismatch, nothing changed. Latest version: {version}.{tail}"),
                true,
            )
        }
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

#[cfg(test)]
mod tests;
