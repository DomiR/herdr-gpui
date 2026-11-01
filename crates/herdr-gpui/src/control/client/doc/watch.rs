//! `doc watch`: every change to a document as it happens, until
//! interrupted or the document is gone. A loop over the bounded `doc diff
//! --wait` request, so nothing over the control socket is left open beyond
//! one bounded wait; a change that lands while another prints is not lost,
//! since each request asks for everything after the last version printed.

use super::super::{Output, exit};
use crate::control::{
    ErrorCode,
    protocol::{
        Response,
        doc::{DocOp, Reply, TodoChangeInfo},
    },
};
use std::{io::Write, time::Duration};

/// How long one request waits for a change before asking again.
const WAIT: u64 = 600;
/// How often a stopped app is asked for again.
const RETRY: Duration = Duration::from_secs(1);
/// How long the app may stay away before the watch gives up.
const GIVE_UP: Duration = Duration::from_secs(600);

/// What `doc watch` was asked for.
pub(crate) struct Watch {
    pub(crate) path: String,
    /// What to print the file as: the path as the caller gave it.
    pub(crate) shown: String,
    pub(crate) since: Option<u64>,
    pub(crate) section: Option<String>,
    pub(crate) todos: bool,
    pub(crate) json: bool,
}

/// The lines of one change: its header, then its diff or task lines.
pub(crate) fn change_text(since: u64, version: u64, by: &[String], body: &Body) -> String {
    let who = if by.is_empty() {
        String::new()
    } else {
        format!(" by {}", by.join(", "))
    };
    let mut text = if version == since + 1 {
        format!("Version {version}{who}")
    } else {
        format!("Versions {}-{version}{who}", since + 1)
    };
    match body {
        Body::Diff(Some(diff)) => {
            text.push('\n');
            text.push_str(diff.trim_end_matches('\n'));
        }
        Body::Diff(None) => {
            text.push_str("\n(the diff is too large; read the document)");
        }
        Body::Todos(todos) => {
            for todo in todos {
                text.push_str(&format!(
                    "\n{} {}. {}",
                    todo.kind.name(),
                    todo.number,
                    todo.text
                ));
            }
        }
    }
    text
}

/// What a change prints under its header.
pub(crate) enum Body {
    Diff(Option<String>),
    Todos(Vec<TodoChangeInfo>),
}

/// Runs the watch: `ask` sends one request, `out` takes the lines, `pause`
/// waits between tries while the app is away. The exit status.
pub(crate) fn run(
    watch: &Watch,
    ask: &mut dyn FnMut(DocOp) -> crate::Result<Response>,
    out: &mut dyn Write,
    pause: &mut dyn FnMut(Duration),
) -> crate::Result<u8> {
    let mut print = |out: &mut dyn Write, text: &str, json: serde_json::Value| {
        let line = if watch.json {
            json.to_string()
        } else {
            super::super::printable(text, 1 << 20)
        };
        writeln!(out, "{line}")?;
        out.flush()?;
        crate::Result::Ok(())
    };
    let mut since = match watch.since {
        Some(since) => since,
        None => {
            let Some(version) = current(watch, ask, out, pause, &mut print)? else {
                return Ok(exit::GONE);
            };
            print(
                out,
                &format!("Watching {} at version {version}", watch.shown),
                serde_json::json!({ "event": "watching", "version": version }),
            )?;
            version
        }
    };
    loop {
        let response = ask(DocOp::Diff {
            path: watch.path.clone(),
            since,
            wait: WAIT,
            section: watch.section.clone(),
            todos: watch.todos,
        });
        match response {
            Ok(Response::Doc {
                reply:
                    Reply::Diff {
                        version,
                        diff,
                        by,
                        todos,
                        ..
                    },
            }) => {
                if diff.as_deref() == Some("") {
                    continue;
                }
                if version < since {
                    // Numbers start again with the app.
                    since = version;
                    continue;
                }
                let body = if watch.todos && diff.is_some() {
                    Body::Todos(todos.clone())
                } else {
                    Body::Diff(diff.clone())
                };
                print(
                    out,
                    &change_text(since, version, &by, &body),
                    serde_json::json!({
                        "event": "change",
                        "since": since,
                        "version": version,
                        "by": by,
                        "diff": diff,
                        "todos": todos,
                    }),
                )?;
                since = version;
            }
            Ok(response) => return Err(super::super::unexpected(response)),
            Err(error) if is_gone(&error) => {
                return gone(watch, out, &mut print);
            }
            Err(error) if is_away(&error) => {
                let Some(version) = current(watch, ask, out, pause, &mut print)? else {
                    return Ok(exit::GONE);
                };
                print(
                    out,
                    &format!(
                        "Herdr UI restarted; watching {} at version {version}",
                        watch.shown
                    ),
                    serde_json::json!({ "event": "reconnected", "version": version }),
                )?;
                since = version;
            }
            Err(error) if is_timeout(&error) => {}
            Err(error) => return Err(error),
        }
    }
}

type Print<'a> = dyn FnMut(&mut dyn Write, &str, serde_json::Value) -> crate::Result<()> + 'a;

/// The document's version now, asking until the app answers. `None` once
/// the document is gone (after printing so).
fn current(
    watch: &Watch,
    ask: &mut dyn FnMut(DocOp) -> crate::Result<Response>,
    out: &mut dyn Write,
    pause: &mut dyn FnMut(Duration),
    print: &mut Print<'_>,
) -> crate::Result<Option<u64>> {
    let mut away = Duration::ZERO;
    loop {
        match ask(DocOp::Sections {
            path: watch.path.clone(),
        }) {
            Ok(Response::Doc {
                reply: Reply::Sections { version, .. },
            }) => return Ok(Some(version)),
            Ok(response) => return Err(super::super::unexpected(response)),
            Err(error) if is_gone(&error) => {
                gone(watch, out, print)?;
                return Ok(None);
            }
            Err(error) if (is_away(&error) || is_timeout(&error)) && away < GIVE_UP => {
                pause(RETRY);
                away += RETRY;
            }
            Err(error) => return Err(error),
        }
    }
}

fn gone(watch: &Watch, out: &mut dyn Write, print: &mut Print<'_>) -> crate::Result<u8> {
    print(
        out,
        &format!("{} was deleted or moved; stopped watching", watch.shown),
        serde_json::json!({ "event": "gone" }),
    )?;
    Ok(exit::GONE)
}

fn rejected(error: &crate::Error) -> Option<ErrorCode> {
    match error {
        crate::Error::Control(crate::control::Error::Rejected { code, .. }) => Some(*code),
        _ => None,
    }
}

fn is_gone(error: &crate::Error) -> bool {
    rejected(error) == Some(ErrorCode::NotFound)
}

fn is_timeout(error: &crate::Error) -> bool {
    rejected(error) == Some(ErrorCode::Timeout)
}

/// The app is not running, or went away mid-request, as when it restarts.
fn is_away(error: &crate::Error) -> bool {
    matches!(
        error,
        crate::Error::Control(
            crate::control::Error::Unavailable { .. }
                | crate::control::Error::NoResponse
                | crate::control::Error::Io(_)
        )
    )
}

/// The watch's last word for `finish`: nothing more to print, its status.
pub(crate) fn ended(status: u8) -> Output {
    let mut output = Output::message("");
    output.streamed = true;
    output.status = status;
    output
}

#[cfg(test)]
mod tests;
