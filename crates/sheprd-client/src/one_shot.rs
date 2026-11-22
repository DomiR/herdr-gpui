//! Claude, run once on its own: it reads what it is given on its input and
//! answers in a line, for a chat's summary on a phone or a suggested name
//! for a chat, tab, or workspace. Nothing is sent to any chat or terminal,
//! so their sessions, history, and context are untouched.
//!
//! Blocking: run it on a worker thread, never on a UI thread.

mod error;

pub use error::Error;

use std::{
    ffi::OsString,
    io::{Read, Write},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// How long the agent may take to answer before it is stopped.
pub const WAIT: Duration = Duration::from_secs(90);
/// The longest answer kept, in characters.
pub const MAX_ANSWER_CHARS: usize = 280;
/// The most of the agent's answer read, in bytes; the rest is dropped.
const MAX_OUTPUT_BYTES: u64 = 16 * 1024;
/// How often a running agent is checked for having exited.
const POLL: Duration = Duration::from_millis(100);
/// How long the agent's output may take to close once it has exited.
const DRAIN: Duration = Duration::from_secs(2);

/// Claude's arguments for one answer to `prompt`: no tools, no saved
/// session, only the user's own settings, and no MCP server. Every request
/// is answered by Claude, whatever agent the chat or tab runs: Codex and
/// Gemini cannot be run without tools or MCP servers that reach past the
/// answer.
pub fn claude_args(prompt: &str) -> Vec<OsString> {
    [
        "-p",
        "--tools",
        "",
        "--no-session-persistence",
        "--setting-sources",
        "user",
        "--strict-mcp-config",
        "--output-format",
        "text",
        prompt,
    ]
    .iter()
    .map(OsString::from)
    .collect()
}

/// Blocking: Claude's answer to `prompt` about `input`, on one line and cut
/// to [`MAX_ANSWER_CHARS`]. Setting `cancelled` stops Claude and what it
/// started, as when nobody waits for the answer any more.
pub fn ask(prompt: &str, input: &str, cancelled: &AtomicBool) -> Result<String, Error> {
    run_first(
        [("claude".into(), claude_args(prompt))],
        input,
        WAIT,
        cancelled,
    )
}

/// The answer of the first of `candidates` that gives one, each run for at
/// most `wait`. One not installed, failing, or answering nothing (an agent
/// signed out, say) passes to the next; one that overruns is killed and
/// reaped here, and ends the search, so a request never waits for two.
/// One stopped by `cancelled` ends it too.
pub fn run_first(
    candidates: impl IntoIterator<Item = (OsString, Vec<OsString>)>,
    input: &str,
    wait: Duration,
    cancelled: &AtomicBool,
) -> Result<String, Error> {
    let mut last = Error::NoAgent;
    for (program, args) in candidates {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        match run_one(program, args, input, wait, cancelled) {
            Err(error @ (Error::NoAgent | Error::Failed | Error::Empty)) => last = error,
            answer => return answer,
        }
    }
    Err(last)
}

/// `program`'s answer, or [`Error::NoAgent`] when it is not installed.
fn run_one(
    program: OsString,
    args: Vec<OsString>,
    input: &str,
    wait: Duration,
    cancelled: &AtomicBool,
) -> Result<String, Error> {
    // A fresh folder only this user can enter, removed after the run: a
    // shared one such as Linux's `/tmp` could hold another user's agent
    // settings, hooks, or MCP servers, which the agent would load.
    let folder = tempfile::Builder::new()
        .prefix("sheprd-agent-")
        .tempdir()
        .map_err(Error::Start)?;
    let mut command = crate::new_command(&program);
    // A user service's or a desktop app's `PATH` is bare; the login
    // shell's finds the agents the user's own shell runs.
    crate::login_env::apply(&mut command);
    // Its own process group, so what it starts ends with it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    // Away from any checkout, so the agent reads only what it is given.
    command
        .args(args)
        .current_dir(folder.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match command.spawn() {
        Ok(child) => answer(child, input.to_owned(), wait, cancelled),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error::NoAgent),
        Err(error) => Err(Error::Start(error)),
    }
}

fn answer(
    mut child: std::process::Child,
    input: String,
    wait: Duration,
    cancelled: &AtomicBool,
) -> Result<String, Error> {
    if let Some(mut stdin) = child.stdin.take() {
        // A failed write shows as the agent's own failure or empty answer.
        thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let (sender, output) = mpsc::sync_channel(1);
    if let Some(stdout) = child.stdout.take() {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut limited = stdout.take(MAX_OUTPUT_BYTES);
            let _ = limited.read_to_end(&mut bytes);
            // The rest is drained so the agent never blocks on a full pipe.
            let _ = std::io::copy(&mut limited.into_inner(), &mut std::io::sink());
            let _ = sender.send(bytes);
        });
    }
    let deadline = Instant::now() + wait;
    let status = loop {
        let stop = match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if cancelled.load(Ordering::Acquire) => Error::Cancelled,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(POLL);
                continue;
            }
            Ok(None) | Err(_) => Error::TimedOut,
        };
        end_group(&child);
        let _ = child.kill();
        let _ = child.wait();
        return Err(stop);
    };
    // What it started and left running would hold the output open.
    end_group(&child);
    let bytes = output.recv_timeout(DRAIN).unwrap_or_default();
    if !status.success() {
        return Err(Error::Failed);
    }
    let text =
        sheprd_chat_protocol::excerpt::one_line(&String::from_utf8_lossy(&bytes), MAX_ANSWER_CHARS);
    if text.is_empty() {
        return Err(Error::Empty);
    }
    Ok(text)
}

/// Kills what is left of the agent's process group. Windows has none here;
/// the bounded wait for the output keeps a leftover child from holding
/// the run there.
fn end_group(child: &std::process::Child) {
    #[cfg(unix)]
    if let Some(group) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    #[cfg(not(unix))]
    let _ = child;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
