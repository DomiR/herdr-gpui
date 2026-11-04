//! The channel behind `herdr-gpui <command>`: a Unix socket, or a named
//! pipe on Windows. A worker accepts connections and serves each on a thread
//! of its own. The short bound covers every connection while it is read, and
//! requests that wait on purpose move to a larger bound after decoding.
mod admission;
#[cfg(windows)]
mod pipe;
#[cfg(unix)]
mod unix;

use crate::protocol::{ErrorCode, FeedbackRequest, MAX_MESSAGE, Request, Response};
use admission::{Permit, Pool};
#[cfg(windows)]
use pipe as transport;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};
use transport::Connection;
#[cfg(unix)]
use unix as transport;

const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a caller waits for the UI, which may be busy or have no window.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// Ordinary requests and every connection while its request is read.
const SHORT_CONNECTIONS: usize = 8;
/// Requests that wait on purpose, keeping short requests available.
const WAIT_CONNECTIONS: usize = 32;
/// Connections served at once: the most requests the UI can be asked to hold.
pub const MAX_CONNECTIONS: usize = SHORT_CONNECTIONS + WAIT_CONNECTIONS;
/// Each admitted connection holds at most one entry, so a full queue never
/// refuses an admitted request.
const QUEUE: usize = MAX_CONNECTIONS;

/// Where this user's app listens, given its state directory.
pub fn address(state: &Path) -> crate::Result<PathBuf> {
    transport::address(state)
}

/// How long the UI may take to answer `request`: one that waits for the
/// user, a page, or an event on purpose gets as long as it asked for.
fn answer_timeout(request: &Request) -> Duration {
    ANSWER_TIMEOUT + Duration::from_secs(request.wait_seconds())
}

/// A request waiting for the UI's answer.
pub struct Incoming {
    pub request: Request,
    reply: Arc<Mutex<Reply>>,
}

struct Reply {
    live: bool,
    sender: SyncSender<Response>,
}

pub enum Event {
    Request(Incoming),
    Undelivered {
        request: FeedbackRequest,
        text: String,
    },
}

impl Incoming {
    pub fn respond(self, response: Response) {
        let _ = self.try_respond(response);
    }

    pub fn is_live(&self) -> bool {
        self.reply
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
    }

    /// Return ownership if the worker stopped waiting, so notes can be kept.
    pub fn try_respond(self, response: Response) -> Result<(), Response> {
        let reply = self
            .reply
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !reply.live {
            return Err(response);
        }
        reply
            .sender
            .try_send(response)
            .map_err(|error| match error {
                TrySendError::Full(response) | TrySendError::Disconnected(response) => response,
            })
    }
}

pub struct Server {
    path: PathBuf,
    requests: Receiver<Event>,
}

impl Server {
    /// Listens at `path`. Another live server keeps it and this one does
    /// not start.
    pub fn bind(path: &Path) -> crate::Result<Self> {
        let listener = transport::Listener::bind(path)?;
        let (sender, requests) = mpsc::sync_channel(QUEUE);
        // The worker blocks in accept for the life of the process. Nothing
        // joins it: the UI must never wait on it, and exit reclaims it.
        thread::Builder::new()
            .name("control-socket".into())
            .spawn(move || {
                let short = Pool::new(SHORT_CONNECTIONS);
                let waits = Pool::new(WAIT_CONNECTIONS);
                loop {
                    let mut stream = match listener.accept() {
                        Ok(stream) => stream,
                        Err(error) => {
                            tracing::debug!(%error, "Control accept failed");
                            // A failure that repeats, such as a pipe instance
                            // that cannot be made, must not spin this thread.
                            thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                    };
                    if let Err(error) = stream.prepare(IO_TIMEOUT) {
                        tracing::debug!(%error, "Control connection setup failed");
                        continue;
                    }
                    let Some(permit) = short.try_acquire() else {
                        refuse(&stream);
                        continue;
                    };
                    let (queue, waits) = (sender.clone(), waits.clone());
                    let spawned = thread::Builder::new()
                        .name("control-connection".into())
                        .spawn(move || {
                            serve(&stream, &queue, permit, &waits);
                        });
                    if let Err(error) = spawned {
                        tracing::debug!(%error, "Control connection thread failed");
                    }
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            requests,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Requests that arrived since the last call.
    pub fn drain(&self) -> impl Iterator<Item = Event> + '_ {
        self.requests.try_iter()
    }

    /// The next request, waiting up to five seconds: for tests that play
    /// the UI.
    #[cfg(any(test, feature = "test-support"))]
    pub fn next(&self) -> Option<Incoming> {
        match self.requests.recv_timeout(Duration::from_secs(5)).ok()? {
            Event::Request(incoming) => Some(incoming),
            Event::Undelivered { .. } => None,
        }
    }
}

/// Cleans up after the server at `path` when the app quits.
pub fn release(path: &Path) {
    transport::release(path);
}

/// Reads one line of at most `MAX_MESSAGE` bytes, without its newline.
fn read_line(reader: impl Read) -> crate::Result<Vec<u8>> {
    let mut line = Vec::new();
    BufReader::new(reader.take(MAX_MESSAGE as u64 + 1)).read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
    } else if line.len() > MAX_MESSAGE {
        return Err(crate::Error::MessageSize { limit: MAX_MESSAGE });
    }
    Ok(line)
}

fn write_line(mut writer: impl Write, message: &impl serde::Serialize) -> crate::Result<()> {
    let mut bytes = serde_json::to_vec(message)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

fn answer(request: Request, queue: &SyncSender<Event>, stream: &Connection) -> Response {
    let timeout = answer_timeout(&request);
    let (sender, answer) = mpsc::sync_channel(1);
    let reply = Arc::new(Mutex::new(Reply { live: true, sender }));
    match queue.try_send(Event::Request(Incoming {
        request,
        reply: reply.clone(),
    })) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            return Response::error(ErrorCode::Busy, "Herdr UI is handling other requests");
        }
        Err(TrySendError::Disconnected(_)) => {
            return Response::error(ErrorCode::NoWindow, "Herdr UI is shutting down");
        }
    }
    let until = Instant::now() + timeout;
    loop {
        if let Ok(response) = answer.recv_timeout(Duration::from_millis(50)) {
            return response;
        }
        if Instant::now() >= until || stream.peer_closed() {
            // No I/O under this lock. Serialize cancellation with try_respond,
            // retaining a response that raced the disconnect instead of dropping it.
            let mut reply = reply
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            reply.live = false;
            return answer.try_recv().unwrap_or_else(|_| {
                Response::error(ErrorCode::Timeout, "Herdr UI did not answer in time")
            });
        }
    }
}

/// Answers a connection over the bound without reading it.
fn refuse(stream: &Connection) {
    let busy = Response::error(ErrorCode::Busy, "Herdr UI is handling other requests");
    if let Err(error) = write_line(stream, &busy) {
        tracing::debug!(%error, "Control refusal failed");
    }
}

/// Serves one request on a connection whose timeouts are already set.
fn serve(stream: &Connection, queue: &SyncSender<Event>, permit: Permit, waits: &Pool) {
    let request = read_line(stream).and_then(|line| Ok(serde_json::from_slice::<Request>(&line)?));
    let (request, response, _permit) = match request {
        Ok(request) => {
            if let Err(error) = stream.watch(true) {
                tracing::debug!(%error, "Control disconnect monitoring failed");
                return;
            }
            // Held until the answer is written. A long wait trades its short
            // permit for a wait permit, taken before the short one is dropped.
            let permit = if request.long_wait() {
                let Some(wait) = waits.try_acquire() else {
                    finish_response(
                        stream,
                        queue,
                        Some(request),
                        Response::error(ErrorCode::Busy, "Too many requests are waiting"),
                    );
                    return;
                };
                drop(permit);
                wait
            } else {
                permit
            };
            let response = answer(request.clone(), queue, stream);
            (Some(request), response, permit)
        }
        Err(error) => (
            None,
            Response::error(ErrorCode::InvalidRequest, error.to_string()),
            permit,
        ),
    };
    finish_response(stream, queue, request, response);
}

/// Whether an answer fits the bound the caller reads with, newline included.
fn fits(response: &Response) -> bool {
    serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() < MAX_MESSAGE)
}

fn finish_response(
    stream: &Connection,
    queue: &SyncSender<Event>,
    request: Option<Request>,
    response: Response,
) {
    // A feature that would send more is refused here rather than cut off
    // mid-line. Notes that cannot go out are kept again, as on any failed write.
    let response = if fits(&response) {
        response
    } else {
        if let (Some(Request::Feedback(request)), Response::Feedback { text: Some(text) }) =
            (&request, response)
        {
            let _ = queue.send(Event::Undelivered {
                request: request.clone(),
                text,
            });
        }
        tracing::warn!("Control answer over {MAX_MESSAGE} bytes refused");
        Response::error(
            ErrorCode::Failed,
            format!("Herdr UI's answer is larger than {MAX_MESSAGE} bytes"),
        )
    };
    let written = stream
        .watch(false)
        .map_err(crate::Error::from)
        .and_then(|()| write_line(stream, &response));
    if let Err(error) = written {
        tracing::debug!(%error, "Control response failed");
        if let (Some(Request::Feedback(request)), Response::Feedback { text: Some(text) }) =
            (request, response)
        {
            // Bounded backpressure is safe here, off the UI thread. The queue
            // and connection limits also bound notes awaiting their return.
            let _ = queue.send(Event::Undelivered { request, text });
        }
    }
}

/// Sends one request to the server at `path` and returns its answer.
pub fn call(path: &Path, request: &Request) -> crate::Result<Response> {
    // The server waits this long for the UI before answering.
    let stream = transport::connect(path, IO_TIMEOUT, answer_timeout(request) + IO_TIMEOUT)?;
    exchange(&stream, &stream, request)
}

fn exchange(
    mut writer: impl Write,
    reader: impl Read,
    request: &Request,
) -> crate::Result<Response> {
    if let Err(error) = write_line(&mut writer, request) {
        let crate::Error::Io(io_error) = &error else {
            return Err(error);
        };
        if matches!(
            io_error.kind(),
            std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::WouldBlock
        ) {
            return Err(error);
        }
        // Try the refusal line for every other write error: Darwin reports a
        // close as EPIPE, ENOTCONN, or EPROTOTYPE, and a dead peer makes the
        // read return EOF at once, so trying costs nothing. `write_all`
        // already retries Interrupted.
        if let Ok(line) = read_line(reader)
            && let Ok(
                response @ Response::Error {
                    code: ErrorCode::Busy,
                    ..
                },
            ) = serde_json::from_slice::<Response>(&line)
        {
            return Ok(response);
        }
        return Err(error);
    }
    let line = read_line(reader)?;
    if line.is_empty() {
        return Err(crate::Error::NoResponse);
    }
    Ok(serde_json::from_slice(&line)?)
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;
#[cfg(any(test, feature = "test-support"))]
pub use test_support::address as test_address;
