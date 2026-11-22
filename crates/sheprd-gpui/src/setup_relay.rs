//! Device setup runs on an app-owned terminal so SSH inherits the app's macOS
//! Local Network permission. Add Device consumes its bounded output and sends
//! explicit responses through `gui`; no daemon workspace is involved.
//! The attach helper remains available for previously launched setups.
// i18n: migrated

mod attach;
mod error;
mod frame;
pub(crate) mod gui;
mod host;
mod session;

pub(crate) use error::Error;
pub(crate) use host::{Listening, listen};

use std::{ffi::OsString, path::Path, process::ExitCode};

/// The argument that makes this executable the pane's end of a setup.
pub(crate) const ATTACH: &str = "__device-setup-attach";
/// The argument that makes this executable the first process of a setup.
pub(crate) const SESSION: &str = "__device-setup-session";

/// Runs this executable as one end of a setup when `args` asks for it, as
/// the updater's helper does, before anything of the app starts.
pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    let (first, rest) = args.split_first()?;
    if first == ATTACH {
        return Some(match rest {
            [path] if !path.is_empty() => attach::run(Path::new(path)),
            _ => usage(ATTACH, "SOCKET"),
        });
    }
    if first == SESSION {
        return Some(match rest.split_first() {
            Some((program, args)) => session::run(program, args),
            None => usage(SESSION, "PROGRAM [ARGS...]"),
        });
    }
    None
}

fn usage(helper: &str, arguments: &str) -> ExitCode {
    eprintln!("Usage: sheprd {helper} {arguments}"); // i18n: data
    ExitCode::from(2)
}

#[cfg(test)]
mod tests;
