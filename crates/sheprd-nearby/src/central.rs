//! The scanning side, for adding another computer's Sheprd that is on
//! another network (`docs/relay-plan.md`, Adding a nearby Sheprd from
//! another network): this computer plays the phone. It looks for
//! computers advertising the nearby service, reads each one's `info`,
//! lists those that name themselves (only while their Pair device is
//! open), and carries `pair` frames to the one the user picks, framed as a
//! phone frames them. As on the peripheral side it holds no secrets and
//! decides nothing: the code exchange runs in `sheprd-remote-control
//! desktop-pair`, and Sheprd carries its frames.
//!
//! [`Central`] is the whole behavior of `macos/sheprd-nearby/Central.swift`
//! as a state machine without a radio: Sheprd's commands and the
//! platform's reports go in, lines for Sheprd and [`Action`]s for the radio
//! come out. [`run`] drives it on a thread of its own, and each platform's
//! [`Link`] only turns actions into BlueZ or WinRT calls and their results
//! back into [`Input`]s.

#[cfg(any(target_os = "linux", test))]
pub(crate) mod latest;

use crate::{
    Result,
    framing::{self, MIN_CHUNK, Outcome, Reassembly},
    ids::{self, Ids},
    protocol::{Event, Out, Radio},
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    time::{Duration, Instant},
};

/// How often `info` is read again from every computer found while
/// scanning: one seen before its Pair device opened names itself only
/// then, and one whose dialog closed stops naming itself.
pub(crate) const REFRESH: Duration = Duration::from_secs(3);
/// The longest name or fingerprint passed on to Sheprd, in characters.
const MAX_NAME: usize = 64;

/// What Sheprd asks of the scanning side.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Command {
    /// Starts or stops looking for computers showing Pair device. Stopping
    /// leaves alone the one being paired with.
    Scan(bool),
    /// Gets the computer Sheprd calls this ready to carry `pair` frames;
    /// `connected` follows.
    Connect(String),
    /// Lets go of the computer being paired with.
    Disconnect(String),
    /// A message for the computer `peripheral`.
    Write { peripheral: String, data: Vec<u8> },
}

/// What the platform reports, naming each computer by its own key (a BlueZ
/// device path, a Bluetooth address on Windows).
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Input {
    Radio(Radio),
    /// A computer advertising the nearby service was seen.
    Discovered(String),
    /// The link to the computer is up.
    Connected(String),
    /// The nearby service was looked up: which of its characteristics the
    /// computer has, and the largest write it takes.
    Resolved {
        device: String,
        info: bool,
        pair: bool,
        chunk: usize,
    },
    /// What `info` read, or `None` when the read was refused or failed,
    /// for the [`Action::ReadInfo`] of the same `attempt`.
    Info {
        device: String,
        attempt: u64,
        value: Option<Vec<u8>>,
    },
    /// Whether `pair` now notifies, after a subscribe or unsubscribe.
    Subscribed {
        device: String,
        notifying: bool,
    },
    /// A chunk the computer notified on `pair`.
    Notified {
        device: String,
        value: Vec<u8>,
    },
    /// The last write finished; `failure` says why it did not, for the log.
    Written {
        device: String,
        failure: Option<String>,
    },
    /// The link failed or ended.
    Disconnected(String),
    /// The radio refused the [`Action::Discover`] asking for `on`.
    DiscoveryFailed {
        on: bool,
        failure: String,
    },
}

/// What the radio does next, for the platform's [`Link`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    /// Starts or stops discovery, filtered to the nearby service.
    Discover(bool),
    Connect(String),
    /// Looks up the nearby service and its `info` and `pair`.
    Resolve(String),
    /// Reads `info`; the answer carries `attempt` back, so one from an
    /// earlier connection to the computer is told apart.
    ReadInfo {
        device: String,
        attempt: u64,
    },
    Subscribe {
        device: String,
        on: bool,
    },
    /// One chunk, written with response; the next waits for [`Input::Written`].
    Write {
        device: String,
        chunk: Vec<u8>,
    },
    Disconnect(String),
}

/// One thing [`Central`] wants done, in order.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Step {
    Emit(Event),
    Act(Action),
}

/// A computer connected to read its `info`, or to pair.
#[derive(Debug)]
struct Peer {
    /// Which connection to the computer this is: reads started for an
    /// earlier one answer with an older number.
    attempt: u64,
    connected: bool,
    info: bool,
    /// Whether its `pair` characteristic is known, so frames can go.
    pair: bool,
    chunk: usize,
    incoming: Reassembly,
    /// Chunks waiting for the write before them to be acknowledged.
    pending: VecDeque<Vec<u8>>,
}

impl Default for Peer {
    fn default() -> Self {
        Self {
            attempt: 0,
            connected: false,
            info: false,
            pair: false,
            chunk: MIN_CHUNK,
            incoming: Reassembly::default(),
            pending: VecDeque::new(),
        }
    }
}

#[derive(Deserialize)]
struct Info {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    fp: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Central {
    scanning: bool,
    /// Whether the radio was asked to discover; set back when it refuses.
    discovering: bool,
    /// The last [`Peer::attempt`] given out.
    attempts: u64,
    radio: Option<Radio>,
    peers: HashMap<String, Peer>,
    /// The computers to subscribe to once their characteristics are known.
    wanted: HashSet<String>,
    /// The computers Sheprd was told about with `found`, which it is told
    /// about again when they leave.
    listed: HashSet<String>,
    ids: Ids,
}

impl Central {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            scanning: false,
            discovering: false,
            attempts: 0,
            radio: None,
            peers: HashMap::new(),
            wanted: HashSet::new(),
            listed: HashSet::new(),
            ids: Ids::new(seed),
        }
    }

    pub(crate) fn scanning(&self) -> bool {
        self.scanning
    }

    /// The id Sheprd knows the computer `device` by.
    #[cfg(test)]
    pub(crate) fn id(&mut self, device: &str) -> String {
        self.ids.id(device)
    }

    /// Records the radio without telling Sheprd, which hears it with the
    /// next `scan`.
    pub(crate) fn unavailable(&mut self) {
        self.radio = Some(Radio::Unsupported);
    }

    pub(crate) fn command(&mut self, command: Command) -> Vec<Step> {
        let mut steps = Vec::new();
        match command {
            Command::Scan(on) => self.scan(on, &mut steps),
            Command::Connect(id) => self.connect(&id, &mut steps),
            Command::Disconnect(id) => self.disconnect(&id, &mut steps),
            Command::Write { peripheral, data } => self.write(&peripheral, &data, &mut steps),
        }
        steps
    }

    fn scan(&mut self, on: bool, steps: &mut Vec<Step>) {
        self.scanning = on;
        // A page opened again learns the radio at once, not at its next
        // change.
        if on && let Some(state) = self.radio {
            steps.push(Step::Emit(Event::CentralState { state }));
        }
        self.apply(steps);
        if on {
            return;
        }
        let gone: Vec<String> = self
            .peers
            .keys()
            .filter(|device| !self.wanted.contains(*device))
            .cloned()
            .collect();
        for device in gone {
            steps.push(Step::Act(Action::Disconnect(device.clone())));
            self.left(&device, steps);
            self.peers.remove(&device);
        }
    }

    fn connect(&mut self, id: &str, steps: &mut Vec<Step>) {
        let Some((device, peer)) = self
            .ids
            .device(id)
            .and_then(|device| self.peers.get_key_value(device))
        else {
            steps.push(Step::Emit(Event::PeripheralDisconnected {
                peripheral: id.to_owned(),
            }));
            return;
        };
        let device = device.clone();
        let action = if peer.pair {
            Action::Subscribe {
                device: device.clone(),
                on: true,
            }
        } else if peer.connected {
            Action::Resolve(device.clone())
        } else {
            Action::Connect(device.clone())
        };
        self.wanted.insert(device);
        steps.push(Step::Act(action));
    }

    fn disconnect(&mut self, id: &str, steps: &mut Vec<Step>) {
        let Some(device) = self.ids.device(id).map(str::to_owned) else {
            return;
        };
        self.wanted.remove(&device);
        let Some(peer) = self.peers.get_mut(&device) else {
            return;
        };
        if self.scanning {
            // It stays listed while scanning, as another attempt may pick
            // it again.
            if peer.pair {
                steps.push(Step::Act(Action::Subscribe {
                    device: device.clone(),
                    on: false,
                }));
            }
            peer.incoming = Reassembly::default();
            peer.pending.clear();
        } else {
            steps.push(Step::Act(Action::Disconnect(device.clone())));
            self.listed.remove(&device);
            self.peers.remove(&device);
        }
    }

    fn write(&mut self, id: &str, data: &[u8], steps: &mut Vec<Step>) {
        let Some(device) = self.ids.device(id).map(str::to_owned) else {
            return;
        };
        let Some(peer) = self.peers.get_mut(&device).filter(|peer| peer.pair) else {
            return;
        };
        let idle = peer.pending.is_empty();
        peer.pending.extend(framing::chunks(data, peer.chunk));
        if idle && let Some(chunk) = peer.pending.front() {
            steps.push(Step::Act(Action::Write {
                device,
                chunk: chunk.clone(),
            }));
        }
    }

    pub(crate) fn input(&mut self, input: Input) -> Vec<Step> {
        let mut steps = Vec::new();
        match input {
            Input::Radio(state) => {
                self.radio = Some(state);
                if state != Radio::PoweredOn {
                    // A radio that went off stopped discovering with it.
                    self.discovering = false;
                }
                steps.push(Step::Emit(Event::CentralState { state }));
                self.apply(&mut steps);
            }
            Input::Discovered(device) => {
                if self.scanning && !self.peers.contains_key(&device) {
                    self.attempts += 1;
                    let peer = Peer {
                        attempt: self.attempts,
                        ..Peer::default()
                    };
                    self.peers.insert(device.clone(), peer);
                    steps.push(Step::Act(Action::Connect(device)));
                }
            }
            Input::Connected(device) => {
                if let Some(peer) = self.peers.get_mut(&device) {
                    peer.connected = true;
                    steps.push(Step::Act(Action::Resolve(device)));
                }
            }
            Input::Resolved {
                device,
                info,
                pair,
                chunk,
            } => self.resolved(device, info, pair, chunk, &mut steps),
            Input::Info {
                device,
                attempt,
                value,
            } => self.info(&device, attempt, value.as_deref(), &mut steps),
            Input::Subscribed { device, notifying } => {
                if self.wanted.contains(&device) {
                    let peripheral = self.ids.id(&device);
                    steps.push(Step::Emit(if notifying {
                        Event::Connected { peripheral }
                    } else {
                        Event::PeripheralDisconnected { peripheral }
                    }));
                }
            }
            Input::Notified { device, value } => self.notified(&device, &value, &mut steps),
            Input::Written { device, failure } => self.written(device, failure, &mut steps),
            Input::Disconnected(device) => {
                self.left(&device, &mut steps);
                self.peers.remove(&device);
            }
            Input::DiscoveryFailed { on, failure } => {
                // Only the latest ask counts: an earlier one refused after
                // another was made changes nothing. Recording it as not done
                // lets the next refresh or scan ask again.
                if self.discovering == on {
                    self.discovering = !on;
                }
                steps.push(Step::Emit(Event::error(format_args!(
                    "discovery: {failure}"
                ))));
            }
        }
        steps
    }

    fn resolved(
        &mut self,
        device: String,
        info: bool,
        pair: bool,
        chunk: usize,
        steps: &mut Vec<Step>,
    ) {
        let Some(peer) = self.peers.get_mut(&device) else {
            return;
        };
        if !info && !pair {
            // Not the nearby service after all: let it go.
            steps.push(Step::Act(Action::Disconnect(device.clone())));
            self.left(&device, steps);
            self.peers.remove(&device);
            return;
        }
        peer.connected = true;
        peer.info = info;
        peer.pair = pair;
        peer.chunk = chunk.max(MIN_CHUNK);
        if info {
            steps.push(Step::Act(Action::ReadInfo {
                device: device.clone(),
                attempt: peer.attempt,
            }));
        }
        if pair && self.wanted.contains(&device) {
            steps.push(Step::Act(Action::Subscribe { device, on: true }));
        }
    }

    /// Only a computer showing Pair device names itself; one advertising
    /// for presence alone does not, nor does one whose dialog closed, which
    /// refuses the read. A reply for a computer that left since, or from an
    /// earlier connection to it, is stale and changes nothing.
    fn info(&mut self, device: &str, attempt: u64, value: Option<&[u8]>, steps: &mut Vec<Step>) {
        if self
            .peers
            .get(device)
            .is_none_or(|peer| peer.attempt != attempt)
        {
            return;
        }
        let named = value
            .and_then(|value| serde_json::from_slice::<Info>(value).ok())
            .and_then(|info| {
                let name: String = info
                    .name?
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(MAX_NAME)
                    .collect();
                let fp: String = info.fp.unwrap_or_default().chars().take(MAX_NAME).collect();
                (!name.trim().is_empty()).then_some((name, fp))
            });
        let Some((name, fp)) = named else {
            if !self.wanted.contains(device) && self.listed.remove(device) {
                steps.push(Step::Emit(Event::PeripheralDisconnected {
                    peripheral: self.ids.id(device),
                }));
            }
            return;
        };
        if self.listed.insert(device.to_owned()) {
            steps.push(Step::Emit(Event::Found {
                peripheral: self.ids.id(device),
                name,
                fp,
            }));
        }
    }

    fn notified(&mut self, device: &str, value: &[u8], steps: &mut Vec<Step>) {
        let Some(peer) = self.peers.get_mut(device) else {
            return;
        };
        match peer.incoming.add(value) {
            Outcome::Partial => {}
            Outcome::Complete(message) => {
                let id = self.ids.id(device);
                steps.push(Step::Emit(Event::peripheral_frame(&id, &message)));
            }
            Outcome::Invalid => peer.incoming = Reassembly::default(),
        }
    }

    fn written(&mut self, device: String, failure: Option<String>, steps: &mut Vec<Step>) {
        let Some(peer) = self.peers.get_mut(&device) else {
            return;
        };
        if let Some(failure) = failure {
            peer.pending.clear();
            steps.push(Step::Emit(Event::error(format_args!("write: {failure}"))));
            return;
        }
        peer.pending.pop_front();
        if let Some(chunk) = peer.pending.front() {
            steps.push(Step::Act(Action::Write {
                device,
                chunk: chunk.clone(),
            }));
        }
    }

    /// Reads `info` again from every computer found and not being paired,
    /// and asks again for discovery the radio refused.
    pub(crate) fn tick(&mut self) -> Vec<Step> {
        let mut steps = Vec::new();
        if !self.scanning {
            return steps;
        }
        self.apply(&mut steps);
        let mut due: Vec<(&String, u64)> = self
            .peers
            .iter()
            .filter(|(device, peer)| peer.connected && peer.info && !self.wanted.contains(*device))
            .map(|(device, peer)| (device, peer.attempt))
            .collect();
        due.sort();
        steps.extend(due.into_iter().map(|(device, attempt)| {
            Step::Act(Action::ReadInfo {
                device: device.clone(),
                attempt,
            })
        }));
        steps
    }

    /// Tells Sheprd that `device`, if it was listed or being paired, is gone.
    fn left(&mut self, device: &str, steps: &mut Vec<Step>) {
        let listed = self.listed.remove(device);
        if self.wanted.remove(device) || listed {
            steps.push(Step::Emit(Event::PeripheralDisconnected {
                peripheral: self.ids.id(device),
            }));
        }
    }

    /// Asks the radio to discover while scanning and powered on.
    fn apply(&mut self, steps: &mut Vec<Step>) {
        if self.radio == Some(Radio::PoweredOn) && self.scanning != self.discovering {
            self.discovering = self.scanning;
            steps.push(Step::Act(Action::Discover(self.scanning)));
        }
    }
}

/// When the loop next reads `info` again. It is checked after every
/// message as well as on waking, so a queue busy at the deadline delays a
/// refresh by one message at most rather than skipping it.
#[derive(Debug, Default)]
pub(crate) struct Refresh {
    next: Option<Instant>,
}

impl Refresh {
    /// How long the loop may wait for a message.
    pub(crate) fn wait(&self, now: Instant) -> Duration {
        self.next.map_or(Duration::from_secs(3600), |at| {
            at.saturating_duration_since(now)
        })
    }

    /// Whether a refresh is due at `now`, scheduling the one after it.
    /// The first comes a full [`REFRESH`] after scanning starts.
    pub(crate) fn due(&mut self, scanning: bool, now: Instant) -> bool {
        match self.next {
            _ if !scanning => {
                self.next = None;
                false
            }
            Some(at) if at > now => false,
            next => {
                self.next = Some(now + REFRESH);
                next.is_some()
            }
        }
    }
}

/// What the central loop reads: Sheprd's commands and the platform's
/// reports, in one queue so they apply in the order they came.
#[derive(Debug)]
pub(crate) enum Message {
    Command(Command),
    Input(Input),
}

/// Where Sheprd's commands and the platform's reports go.
#[derive(Clone, Debug)]
pub(crate) struct Inbox(Sender<Message>);

impl Inbox {
    pub(crate) fn new(sender: Sender<Message>) -> Self {
        Self(sender)
    }

    pub(crate) fn command(&self, command: Command) {
        let _ = self.0.send(Message::Command(command));
    }

    pub(crate) fn send(&self, input: Input) {
        let _ = self.0.send(Message::Input(input));
    }
}

/// Turns [`Action`]s into the platform's Bluetooth calls; one per platform
/// and a recording one in tests. Slow calls run off the loop and report
/// back through the [`Inbox`] the link was opened with.
pub(crate) trait Link {
    fn act(&self, action: Action);
}

/// Runs the scanning side until its queue ends. The radio is opened with
/// `open` on the first `scan`, so a helper started only for pairing or
/// presence never touches it; one that could not open is tried again at
/// the next `scan`.
pub(crate) fn run<L: Link>(
    messages: &Receiver<Message>,
    inbox: &Inbox,
    out: &Out,
    mut open: impl FnMut(&Inbox) -> Result<L>,
) {
    let mut central = Central::new(ids::random_seed());
    let mut link: Option<L> = None;
    let mut refresh = Refresh::default();
    loop {
        let mut steps = match messages.recv_timeout(refresh.wait(Instant::now())) {
            Ok(Message::Command(command)) => {
                if link.is_none() && command == Command::Scan(true) {
                    match open(inbox) {
                        Ok(opened) => link = Some(opened),
                        Err(error) => {
                            out.emit(&Event::error(format_args!("scanning: {error}")));
                            central.unavailable();
                        }
                    }
                }
                central.command(command)
            }
            Ok(Message::Input(input)) => central.input(input),
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if refresh.due(central.scanning(), Instant::now()) {
            steps.extend(central.tick());
        }
        for step in steps {
            match step {
                Step::Emit(event) => out.emit(&event),
                Step::Act(action) => {
                    if let Some(link) = &link {
                        link.act(action);
                    }
                }
            }
        }
    }
}
