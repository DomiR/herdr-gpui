//! `mod`: the mods the window runs, and checking one before it is installed.
use super::{Args, CliError, unknown};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModCommand {
    List,
    Reload {
        name: String,
    },
    Logs {
        name: String,
        lines: usize,
    },
    /// Load a mod folder and run its view once here, without the app.
    Check {
        dir: PathBuf,
    },
}

pub(super) fn command(args: &mut Args) -> Result<ModCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(ModCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => ModCommand::List,
        Some("reload") => ModCommand::Reload {
            name: args.required_text("NAME")?,
        },
        Some("logs") => ModCommand::Logs {
            lines: args.number("--lines", 1..=200)?.unwrap_or(50),
            name: args.required_text("NAME")?,
        },
        Some("check") => ModCommand::Check {
            dir: args.required("DIR")?.into(),
        },
        _ => return Err(unknown("mod", verb)),
    })
}
