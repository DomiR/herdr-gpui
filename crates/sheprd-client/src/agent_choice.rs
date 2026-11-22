//! Whether a `sheprd-acp` lets its chats switch to another installed agent,
//! read from its `--help`, so the app and the remote control daemon start
//! chats the program accepts.

use std::{path::Path, time::Duration};

/// Lets the chat switch to another installed agent whenever it is idle.
/// Only a program that offers it is asked: one on an SSH host, or an older
/// one found on `PATH` here, would refuse the option and start nothing.
pub const CHOOSE: &str = "--choose";
/// The most of the program's help read to find [`CHOOSE`] in it.
const HELP_LIMIT: u64 = 64 * 1024;
/// How long the program has to print its help: a wrapper that keeps its
/// output open must not leave the question unanswered.
const HELP_DEADLINE: Duration = Duration::from_secs(3);

/// Whether a new chat may switch agents.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AgentChoice {
    /// It keeps the agent it starts with, as a program without
    /// [`CHOOSE`] runs every chat.
    #[default]
    Fixed,
    /// It starts with [`CHOOSE`].
    Open,
}

impl AgentChoice {
    /// The choice `program` offers, from its `--help`. Blocking for up to
    /// [`HELP_DEADLINE`]: run it off the UI thread.
    pub fn of(program: &Path) -> Self {
        Self::of_within(program, HELP_DEADLINE)
    }

    /// [`Self::of`], giving up as [`Self::Fixed`] after `deadline`.
    pub fn of_within(program: &Path, deadline: Duration) -> Self {
        Self::from_help(&help(program, deadline))
    }

    /// The choice a program whose help reads `help` offers.
    pub fn from_help(help: &[u8]) -> Self {
        if lists(help, CHOOSE) {
            Self::Open
        } else {
            Self::Fixed
        }
    }

    /// The option that asks for it, if any.
    pub fn option(self) -> Option<&'static str> {
        match self {
            Self::Fixed => None,
            Self::Open => Some(CHOOSE),
        }
    }
}

/// Whether `program`'s `--help` lists `option`, such as a newer flag an
/// older program would refuse. Blocking for up to [`HELP_DEADLINE`]: run it
/// off the UI thread.
pub fn offers(program: &Path, option: &str) -> bool {
    lists(&help(program, HELP_DEADLINE), option)
}

/// Whether a program whose help reads `help` lists `option`.
fn lists(help: &[u8], option: &str) -> bool {
    help.windows(option.len())
        .any(|word| word == option.as_bytes())
}

/// What `program --help` prints, at most [`HELP_LIMIT`] bytes within
/// `deadline`; nothing when it does not start or answer in time.
fn help(program: &Path, deadline: Duration) -> Vec<u8> {
    use std::io::Read as _;
    let mut command = crate::new_command(program);
    command
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let (sender, receiver) = std::sync::mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        // Read apart, so the deadline holds while the read blocks. A
        // child the program started may keep the pipe open past the
        // kill below; the read then ends when that child does.
        let _ = std::thread::Builder::new()
            .name("acp-help".to_owned())
            .spawn(move || {
                let mut help = Vec::new();
                let _ = stdout.take(HELP_LIMIT).read_to_end(&mut help);
                let _ = sender.send(help);
            });
    }
    let help = receiver.recv_timeout(deadline).unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();
    help
}

#[cfg(test)]
mod tests;
