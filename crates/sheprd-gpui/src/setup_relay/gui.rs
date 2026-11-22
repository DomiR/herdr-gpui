use super::{
    Error, Listening,
    frame::{Frame, MAX_PAYLOAD, Size},
};
use crate::Result;
use secrecy::zeroize::Zeroizing;
use sheprd_terminal::{Emulator, Size as TerminalSize};
use std::{
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::Duration,
};

const ROWS: u16 = 24;
const COLS: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prompt {
    Confirm,
    Response,
}

impl Prompt {
    fn from_output(output: &str) -> Self {
        let line = output
            .lines()
            .last()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if line.contains("[y/n]") || line.contains("(yes/no") {
            Self::Confirm
        } else {
            Self::Response
        }
    }
}

#[derive(Default)]
struct State {
    output: String,
    changed: bool,
    result: Option<Result<i32>>,
}

pub(crate) struct Update {
    pub(crate) output: String,
    pub(crate) result: Option<Result<i32>>,
}

pub(crate) struct Session {
    input: SyncSender<Zeroizing<Vec<u8>>>,
    cancelled: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}

impl Session {
    #[cfg(test)]
    pub(crate) fn fixture() -> Self {
        Self::fixture_state("", None)
    }

    #[cfg(test)]
    pub(crate) fn fixture_state(output: &str, result: Option<Result<i32>>) -> Self {
        let (input, _) = mpsc::sync_channel(1);
        Self {
            input,
            cancelled: Arc::default(),
            state: Arc::new(Mutex::new(State {
                output: output.into(),
                changed: true,
                result,
            })),
        }
    }

    pub(crate) fn connect<T: Send + 'static>(mut relay: Listening<T>, held: T) -> Result<Self> {
        let stream = UnixStream::connect(relay.path())?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;
        let reader = stream.try_clone()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(State::default()));
        let (input, responses) = mpsc::sync_channel::<Zeroizing<Vec<u8>>>(1);
        {
            let (state, cancelled) = (state.clone(), cancelled.clone());
            thread::Builder::new()
                .name("sheprd-setup-dialog-output".into())
                .spawn(move || {
                    let result = read_output(reader, &state);
                    let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                    state.result = Some(result);
                    state.changed = true;
                    cancelled.store(true, Ordering::Release);
                })?;
        }
        {
            let cancelled = cancelled.clone();
            thread::Builder::new()
                .name("sheprd-setup-dialog-input".into())
                .spawn(move || {
                    let mut stream = stream;
                    let hello = Frame::Hello {
                        size: Size {
                            rows: ROWS,
                            cols: COLS,
                        },
                        term: Some("dumb".into()),
                    };
                    if hello.write(&mut stream).is_ok() {
                        while !cancelled.load(Ordering::Acquire) {
                            match responses.recv_timeout(Duration::from_millis(50)) {
                                Ok(bytes) => {
                                    if write_response(&mut stream, &bytes).is_err() {
                                        break;
                                    }
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            }
                        }
                    }
                    let _ = stream.shutdown(Shutdown::Both);
                })?;
        }
        relay.keep(held);
        Ok(Self {
            input,
            cancelled,
            state,
        })
    }

    pub(crate) fn respond(&self, response: &str) -> bool {
        if response.len() >= MAX_PAYLOAD || response.chars().any(char::is_control) {
            return false;
        }
        let mut bytes = Zeroizing::new(response.as_bytes().to_vec());
        bytes.push(b'\n');
        self.input.try_send(bytes).is_ok()
    }

    pub(crate) fn poll(&self) -> Option<Update> {
        let mut state = self.state.try_lock().ok()?;
        if !state.changed {
            return None;
        }
        state.changed = false;
        Some(Update {
            output: state.output.clone(),
            result: state.result.take(),
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn write_response(stream: &mut UnixStream, bytes: &[u8]) -> std::io::Result<()> {
    Frame::write_input(bytes, stream)
}

fn read_output(mut reader: UnixStream, state: &Mutex<State>) -> Result<i32> {
    let size = TerminalSize::new(COLS, ROWS).map_err(|_| Error::Frame)?;
    let mut emulator = Emulator::new(size, 0).without_images();
    while let Some(frame) = Frame::read(&mut reader)? {
        match frame {
            Frame::Output(bytes) => {
                emulator.feed(&bytes);
                emulator.end_sync();
                emulator.take_events();
                emulator.take_damage();
                let output = plain_text(&emulator);
                let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                state.output = output;
                state.changed = true;
            }
            Frame::Exit(code) => return Ok(code),
            _ => return Err(Error::Frame.into()),
        }
    }
    Err(Error::Closed.into())
}

fn plain_text(emulator: &Emulator) -> String {
    let frame = emulator.frame();
    let wrapped = emulator.wrapped_rows();
    let mut output = String::new();
    for (index, row) in frame.cells.chunks(usize::from(frame.width)).enumerate() {
        let mut line = String::new();
        for cell in row
            .iter()
            .filter(|cell| !cell.skip && cell.modifier & sheprd_terminal::HIDDEN == 0)
        {
            line.extend(
                cell.symbol
                    .chars()
                    .filter(|character| !character.is_control()),
            );
        }
        if wrapped.get(index).copied().unwrap_or(false) {
            output.push_str(&line);
        } else {
            output.push_str(line.trim_end());
            output.push('\n');
        }
    }
    output.trim_end().to_owned()
}

pub(crate) fn prompt(output: &str) -> Prompt {
    Prompt::from_output(output)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
