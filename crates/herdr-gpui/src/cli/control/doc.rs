//! `doc`: Markdown documents agents read and change through the app.
use super::{Args, CliError, tabs::FileAt, unknown};
use std::{ffi::OsString, path::PathBuf};

/// Text a change carries: given inline, or `-` to read standard input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    Inline(String),
    Stdin,
}

/// The version a change was made against, and the section it works in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub version: u64,
    pub section: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocCommand {
    Open(FileAt),
    Read {
        path: PathBuf,
        section: Option<String>,
    },
    Diff {
        path: PathBuf,
        since: u64,
        /// Seconds to wait for a change, at most 600.
        wait: u64,
    },
    Sections {
        path: PathBuf,
    },
    /// Every change from `since` (or now) on, until interrupted.
    Watch {
        path: PathBuf,
        since: Option<u64>,
        section: Option<String>,
        todos: bool,
    },
    Append {
        path: PathBuf,
        change: Change,
        text: Text,
    },
    Edit {
        path: PathBuf,
        change: Change,
        old: String,
        new: String,
    },
    TodoList {
        path: PathBuf,
        section: Option<String>,
    },
    TodoAdd {
        path: PathBuf,
        change: Change,
        text: Text,
    },
    TodoCheck {
        path: PathBuf,
        change: Change,
        todo: String,
        done: bool,
    },
}

pub(super) fn command(args: &mut Args) -> Result<DocCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "doc",
        what: "open|read|diff|watch|sections|append|edit|todo",
    })?;
    Ok(match verb.to_str() {
        Some("open") => DocCommand::Open(args.required("FILE[:LINE]")?.into()),
        Some("read") => DocCommand::Read {
            section: args.text_option("--section")?,
            path: path(args)?,
        },
        Some("diff") => DocCommand::Diff {
            since: args
                .number("--since", 1..=u64::MAX)?
                .ok_or(CliError::MissingValue("--since"))?,
            wait: args.number("--wait", 0..=600)?.unwrap_or(0),
            path: path(args)?,
        },
        Some("sections") => DocCommand::Sections { path: path(args)? },
        Some("watch") => DocCommand::Watch {
            since: args.number("--since", 0..=u64::MAX)?,
            section: args.text_option("--section")?,
            todos: args.flag("--todos"),
            path: path(args)?,
        },
        Some("append") => {
            let change = change(args)?;
            DocCommand::Append {
                path: path(args)?,
                change,
                text: text(args)?,
            }
        }
        Some("edit") => {
            let change = change(args)?;
            DocCommand::Edit {
                old: raw_option(args, "--old")?,
                new: raw_option(args, "--new")?,
                path: path(args)?,
                change,
            }
        }
        Some("todo") => todo(args)?,
        _ => return Err(unknown("doc", verb)),
    })
}

fn todo(args: &mut Args) -> Result<DocCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "doc",
        what: "todo list|add|check",
    })?;
    Ok(match verb.to_str() {
        Some("list") => DocCommand::TodoList {
            section: args.text_option("--section")?,
            path: path(args)?,
        },
        Some("add") => {
            let change = change(args)?;
            DocCommand::TodoAdd {
                path: path(args)?,
                change,
                text: text(args)?,
            }
        }
        Some("check" | "uncheck") => {
            let done = verb != "uncheck" && !args.flag("--undo");
            let change = change(args)?;
            let path = path(args)?;
            let todo = match text(args)? {
                Text::Inline(todo) => todo,
                Text::Stdin => return Err(CliError::InvalidValue("NUMBER|TEXT")),
            };
            DocCommand::TodoCheck {
                path,
                change,
                todo,
                done,
            }
        }
        _ => return Err(unknown("doc todo", verb)),
    })
}

fn path(args: &mut Args) -> Result<PathBuf, CliError> {
    Ok(args.required("FILE")?.into())
}

/// `--version N` (required) and `--section ID`.
fn change(args: &mut Args) -> Result<Change, CliError> {
    Ok(Change {
        version: args
            .number("--version", 1..=u64::MAX)?
            .ok_or(CliError::MissingValue("--version"))?,
        section: args.text_option("--section")?,
    })
}

/// The text: the words left, or everything after `--` (so it may start
/// with `-`, as a list item does), or `-` alone to read standard input.
fn text(args: &mut Args) -> Result<Text, CliError> {
    if let Some(trailing) = args.trailing() {
        let words = trailing
            .into_iter()
            .map(super::super::args::utf8)
            .collect::<Result<Vec<_>, _>>()?;
        if !words.is_empty() {
            return Ok(Text::Inline(words.join(" ")));
        }
    }
    if args.flag("-") {
        return Ok(Text::Stdin);
    }
    args.words()?
        .map(Text::Inline)
        .ok_or(CliError::MissingArgument {
            command: "doc",
            what: "TEXT",
        })
}

/// `--flag VALUE` whose value may start with `-`, as Markdown list items do.
fn raw_option(args: &mut Args, flag: &'static str) -> Result<String, CliError> {
    let mut all: Vec<OsString> = args.take_all().collect();
    let at = all
        .iter()
        .position(|arg| arg == flag)
        .ok_or(CliError::MissingValue(flag))?;
    all.remove(at);
    if at >= all.len() {
        return Err(CliError::MissingValue(flag));
    }
    let value = all.remove(at);
    args.put_back(all);
    super::super::args::utf8(value)
}
