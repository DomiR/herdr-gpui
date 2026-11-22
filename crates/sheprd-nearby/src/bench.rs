//! `sheprd-nearby bench`: measures Bluetooth LE between two computers, to
//! decide whether it could ever carry SSH as a last resort
//! (`docs/relay-plan.md`, Measuring Bluetooth as a transport). It is not
//! part of pairing and Sheprd never runs it.
//!
//! Both sides drive the helper's own peripheral and central in-process,
//! standing in for Sheprd: `serve` advertises like an open Pair device
//! dialog and answers [`wire`] messages on `pair`; `run` scans for it,
//! connects as Add Device > Nearby Sheprd does, and times echoes, an
//! upload, a download, and minutes of one echo a second, then prints a
//! [`report::Report`].

mod error;
mod options;
mod report;
mod wire;

pub use error::Error;

use crate::{
    Result, backend, central, ids,
    peripheral::{Core, Shared},
    protocol::{Channel, Command, Event, Out, Radio},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use options::{Mode, Plan, USAGE};
use report::{Latency, Report, Stability, Throughput};
use std::{
    process::ExitCode,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};
use wire::{MAX_BODY, Message, Servers};

/// What `serve` puts in `info` where Pair device puts the CA fingerprint.
const FINGERPRINT: &str = "0000000000000000";
/// How long `run` looks for the serving computer.
const FIND: Duration = Duration::from_secs(60);
/// How long the computer gets to be ready for frames.
const READY: Duration = Duration::from_secs(30);
/// How long an echo may take before it counts as lost.
const ECHO_WAIT: Duration = Duration::from_secs(5);
/// How long a download may pause before it counts as over.
const DOWNLOAD_IDLE: Duration = Duration::from_secs(10);
/// An echo's size, tag included.
const ECHO_SIZE: usize = 64;

/// Runs `sheprd-nearby bench ARGS`.
pub(crate) fn main(args: &[String]) -> ExitCode {
    let mode = match Mode::parse(args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("sheprd-nearby bench: {error}");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match mode {
        Mode::Help => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Mode::Serve { name } => serve(&name),
        Mode::Run(plan) => run(&plan).and_then(|report| {
            let json = serde_json::to_string_pretty(&report).map_err(Error::Report)?;
            println!("{json}");
            Ok(())
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sheprd-nearby bench: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Advertises as `name` and answers every message until stopped.
fn serve(name: &str) -> Result<()> {
    let (sink, events) = mpsc::channel();
    let out = Out::to(sink);
    let core: Shared = Arc::new(Mutex::new(Core::new(ids::random_seed())));
    let (commands, received) = mpsc::channel();
    thread::spawn(move || backend::run(true, &core, &received, &out));
    let _ = commands.send(Command::Advertise {
        name: name.to_owned(),
        fingerprint: FINGERPRINT.to_owned(),
    });
    eprintln!("advertising as {name:?}; stop with Ctrl-C");
    let mut servers = Servers::default();
    for event in events {
        match event {
            Event::State { state } => {
                eprintln!("radio: {state:?}");
                if state == Radio::Unsupported {
                    return Err(Error::Unavailable.into());
                }
            }
            Event::Error { message } => eprintln!("error: {message}"),
            Event::Disconnected { central } => servers.forget(&central),
            Event::Frame { central, data } => {
                let Ok(data) = STANDARD.decode(data) else {
                    continue;
                };
                for reply in servers.reply(&central, &data) {
                    let _ = commands.send(Command::Frame {
                        central: central.clone(),
                        channel: Channel::Pair,
                        data: reply,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Finds the serving computer, measures, and lets it go.
fn run(plan: &Plan) -> Result<Report> {
    let (sink, events) = mpsc::channel();
    let inbox = backend::start_central(&Out::to(sink));
    let mut session = Session {
        inbox,
        events,
        peripheral: String::new(),
    };
    session.reach(&plan.name)?;
    eprintln!("echoing {} times", plan.echoes);
    let echo = session.echoes(plan.echoes)?;
    eprintln!("uploading {} bytes", plan.bytes);
    let upload = session.upload(plan.bytes)?;
    eprintln!("downloading {} bytes", plan.bytes);
    let download = session.download(plan.bytes);
    let stability = (plan.minutes > 0).then(|| {
        eprintln!("echoing once a second for {} minutes", plan.minutes);
        session.stability(&plan.name, plan.minutes)
    });
    session
        .inbox
        .command(central::Command::Disconnect(session.peripheral.clone()));
    Ok(Report::new(
        plan.label.clone(),
        plan.name.clone(),
        echo,
        upload,
        download,
        stability,
    ))
}

/// The running side's link to the serving computer.
struct Session {
    inbox: central::Inbox,
    events: Receiver<Event>,
    /// The id the central gave the serving computer.
    peripheral: String,
}

/// What arrived from the serving computer.
enum Got {
    Message(Message),
    Gone,
    Nothing,
}

impl Session {
    /// Scans for `name`, connects, and stops scanning, which would only
    /// share the radio with the measurement.
    fn reach(&mut self, name: &str) -> Result<()> {
        self.inbox.command(central::Command::Scan(true));
        let deadline = Instant::now() + FIND;
        self.peripheral = loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(wait) {
                Ok(Event::CentralState { state }) => {
                    eprintln!("radio: {state:?}");
                    if state == Radio::Unsupported {
                        return Err(Error::Unavailable.into());
                    }
                }
                Ok(Event::Found {
                    peripheral,
                    name: found,
                    ..
                }) if found == name => break peripheral,
                Ok(Event::Error { message }) => eprintln!("error: {message}"),
                Ok(_) => {}
                Err(_) => {
                    return Err(Error::NotFound {
                        name: name.to_owned(),
                        seconds: FIND.as_secs(),
                    }
                    .into());
                }
            }
        };
        eprintln!("found {name:?}; connecting");
        self.inbox
            .command(central::Command::Connect(self.peripheral.clone()));
        let deadline = Instant::now() + READY;
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(wait) {
                Ok(Event::Connected { peripheral }) if peripheral == self.peripheral => break,
                Ok(Event::PeripheralDisconnected { peripheral })
                    if peripheral == self.peripheral =>
                {
                    return Err(Error::Gone.into());
                }
                Ok(_) => {}
                Err(_) => {
                    return Err(Error::NotReady {
                        seconds: READY.as_secs(),
                    }
                    .into());
                }
            }
        }
        self.inbox.command(central::Command::Scan(false));
        Ok(())
    }

    fn send(&self, message: &Message) {
        self.inbox.command(central::Command::Write {
            peripheral: self.peripheral.clone(),
            data: message.encode(),
        });
    }

    /// The next message from the serving computer before `deadline`.
    fn receive(&self, deadline: Instant) -> Got {
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(wait) {
                Ok(Event::PeripheralFrame { peripheral, data })
                    if peripheral == self.peripheral =>
                {
                    if let Some(message) = STANDARD
                        .decode(data)
                        .ok()
                        .and_then(|data| Message::decode(&data))
                    {
                        return Got::Message(message);
                    }
                }
                Ok(Event::PeripheralDisconnected { peripheral })
                    if peripheral == self.peripheral =>
                {
                    return Got::Gone;
                }
                Ok(Event::Error { message }) => eprintln!("error: {message}"),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => return Got::Nothing,
                Err(RecvTimeoutError::Disconnected) => return Got::Gone,
            }
        }
    }

    /// One echo's round trip, `None` when it was lost; stale echoes of
    /// earlier lost ones are skipped by their sequence number.
    fn echo(&self, sequence: u64) -> std::result::Result<Option<Duration>, Error> {
        let mut body = sequence.to_be_bytes().to_vec();
        body.resize(ECHO_SIZE - 1, 0x5A);
        let message = Message::Echo(body);
        let start = Instant::now();
        self.send(&message);
        let deadline = start + ECHO_WAIT;
        loop {
            match self.receive(deadline) {
                Got::Message(answer) if answer == message => return Ok(Some(start.elapsed())),
                Got::Message(_) => {}
                Got::Gone => return Err(Error::Gone),
                Got::Nothing => return Ok(None),
            }
        }
    }

    fn echoes(&self, count: u32) -> Result<Latency> {
        let mut samples = Vec::new();
        let mut lost = 0;
        for sequence in 0..u64::from(count) {
            match self.echo(sequence)? {
                Some(sample) => samples.push(sample),
                None => lost += 1,
            }
        }
        Ok(Latency::of(&samples, lost))
    }

    /// Writes `bytes`, then asks how many arrived; timed to the answer.
    fn upload(&self, bytes: u32) -> Result<Throughput> {
        let asked = u64::from(bytes);
        let mut left = usize::try_from(bytes).unwrap_or(0);
        let start = Instant::now();
        while left > 0 {
            let take = left.min(MAX_BODY);
            self.send(&Message::Sink(vec![0x3C; take]));
            left -= take;
        }
        self.send(&Message::Tally);
        // Generous: as slow as 2 KB/s still finishes.
        let deadline = start + Duration::from_secs(30 + asked / 2000);
        loop {
            match self.receive(deadline) {
                Got::Message(Message::Tallied(sunk)) => {
                    return Ok(Throughput::of(asked, sunk, start.elapsed()));
                }
                Got::Message(_) => {}
                Got::Gone => return Err(Error::Gone.into()),
                Got::Nothing => return Ok(Throughput::of(asked, 0, start.elapsed())),
            }
        }
    }

    /// Asks for `bytes`; timed to the last that arrived.
    fn download(&self, bytes: u32) -> Throughput {
        let asked = u64::from(bytes);
        let size = u16::try_from(MAX_BODY).unwrap_or(u16::MAX);
        let start = Instant::now();
        self.send(&Message::Pull { bytes, size });
        let mut received = 0;
        let mut last = start;
        while received < asked {
            match self.receive(Instant::now() + DOWNLOAD_IDLE) {
                Got::Message(Message::Data(data)) => {
                    received += data.len() as u64;
                    last = Instant::now();
                }
                Got::Message(_) => {}
                Got::Gone | Got::Nothing => break,
            }
        }
        Throughput::of(asked, received, last.duration_since(start))
    }

    /// One echo a second for `minutes`, finding the computer again after
    /// each disconnection.
    fn stability(&mut self, name: &str, minutes: u32) -> Stability {
        let end = Instant::now() + Duration::from_secs(u64::from(minutes) * 60);
        let mut samples = Vec::new();
        let (mut lost, mut disconnects) = (0, 0);
        let mut sequence = u64::from(u32::MAX);
        while Instant::now() < end {
            let tick = Instant::now() + Duration::from_secs(1);
            sequence += 1;
            match self.echo(sequence) {
                Ok(Some(sample)) => samples.push(sample),
                Ok(None) => lost += 1,
                Err(_) => {
                    disconnects += 1;
                    eprintln!("disconnected; finding {name:?} again");
                    while Instant::now() < end && self.reach(name).is_err() {
                        thread::sleep(Duration::from_secs(1));
                    }
                }
            }
            thread::sleep(tick.saturating_duration_since(Instant::now()));
        }
        Stability {
            minutes,
            disconnects,
            latency: Latency::of(&samples, lost),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
