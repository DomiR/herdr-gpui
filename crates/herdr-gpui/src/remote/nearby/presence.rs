//! Presence, Herdr UI's part (`docs/ios-app-plan.md`, "Presence,
//! exactly"): carries each phone's `presence` frames to the daemon,
//! remembers which phone proved which device, and tells the daemon a
//! device is near while its proven phone says it is and the desktop is in
//! use. The proof and the hold run in the daemon; this only decides near.
//! Pure state: the station feeds it the helper's events and sends what it
//! asks for.

use herdr_remote_control::control::{self, DeviceId, Request, Response};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Input older than this means nobody is at the desk.
pub(crate) const IDLE_LIMIT_SECONDS: u64 = 300;
/// How often a near mark is renewed; the daemon lets one lapse after two
/// minutes.
pub(crate) const RENEW: Duration = Duration::from_secs(30);

/// A phone's presence message.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum PhoneFrame {
    Hello { device: String },
    Prove { device: String, signature: String },
    Near { near: bool },
}

/// What to do with a phone's frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Received {
    /// Ask the daemon, then hand its answer to [`PresenceRelay::answered`].
    Ask(Request),
    /// Write this back to the phone at once.
    Reply(Vec<u8>),
    Nothing,
}

#[derive(Debug, Default)]
struct Central {
    /// The device a challenge was asked for, and its nonce once answered.
    challenge: Option<(DeviceId, Option<String>)>,
    /// The device this phone proved it is.
    proven: Option<DeviceId>,
    /// Whether the phone says it is close (its RSSI, its own hysteresis).
    near: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PresenceRelay {
    centrals: HashMap<String, Central>,
    /// Seconds since the desktop's last input, as the helper last said.
    idle: Option<u64>,
    /// What the daemon was last told per device, and when.
    sent: HashMap<DeviceId, (bool, Instant)>,
}

impl PresenceRelay {
    /// A frame phone `central` wrote.
    pub(crate) fn receive(&mut self, central: &str, data: &[u8]) -> Received {
        let Ok(frame) = serde_json::from_slice::<PhoneFrame>(data) else {
            return Received::Reply(error("that message is not one Herdr reads"));
        };
        let state = self.centrals.entry(central.to_owned()).or_default();
        match frame {
            PhoneFrame::Hello { device } => {
                let device = DeviceId(device);
                state.challenge = Some((device.clone(), None));
                Received::Ask(Request::PresenceChallenge { device })
            }
            PhoneFrame::Prove { device, signature } => match &state.challenge {
                Some((asked, Some(nonce))) if asked.0 == device => {
                    Received::Ask(Request::PresenceProve {
                        device: asked.clone(),
                        nonce: nonce.clone(),
                        signature,
                    })
                }
                _ => Received::Reply(error("say hello first")),
            },
            PhoneFrame::Near { near } => {
                state.near = near;
                Received::Nothing
            }
        }
    }

    /// The daemon's answer to what [`Self::receive`] asked, as the phone
    /// reads it.
    pub(crate) fn answered(
        &mut self,
        central: &str,
        result: Result<Response, control::Error>,
    ) -> Vec<u8> {
        let state = self.centrals.entry(central.to_owned()).or_default();
        match result {
            Ok(Response::PresenceChallenge { nonce }) => {
                if let Some((_, slot)) = &mut state.challenge {
                    *slot = Some(nonce.clone());
                }
                json!({ "type": "challenge", "nonce": nonce })
                    .to_string()
                    .into_bytes()
            }
            Ok(Response::PresenceProven { device, addresses }) => {
                state.challenge = None;
                state.proven = Some(device);
                json!({ "type": "proven", "addresses": addresses })
                    .to_string()
                    .into_bytes()
            }
            Ok(Response::Error { message, .. }) => {
                state.challenge = None;
                error(&message)
            }
            Ok(_) => error("the computer gave an answer this relay does not read"),
            Err(failure) => error(&failure.to_string()),
        }
    }

    /// Seconds since the desktop's last input.
    pub(crate) fn idle(&mut self, seconds: u64) {
        self.idle = Some(seconds);
    }

    /// Phone `central` went out of range or closed the connection.
    pub(crate) fn disconnected(&mut self, central: &str) {
        self.centrals.remove(central);
    }

    /// The near marks to send now: each change, and a renewal every
    /// [`RENEW`] while near. A device is near while any of its proven
    /// phones says so and the desktop is in use.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<Request> {
        let at_desk = self.idle.is_some_and(|idle| idle < IDLE_LIMIT_SECONDS);
        let mut wanted: HashMap<DeviceId, bool> = self
            .sent
            .keys()
            .map(|device| (device.clone(), false))
            .collect();
        for central in self.centrals.values() {
            if let Some(device) = &central.proven {
                let near = wanted.entry(device.clone()).or_insert(false);
                *near |= central.near && at_desk;
            }
        }
        let mut requests = Vec::new();
        for (device, near) in wanted {
            let send = match self.sent.get(&device) {
                None => near,
                Some((sent, at)) => *sent != near || (near && now.duration_since(*at) >= RENEW),
            };
            if send {
                requests.push(Request::PresenceSet {
                    device: device.clone(),
                    near,
                });
            }
            if near {
                if send {
                    self.sent.insert(device, (near, now));
                }
            } else {
                self.sent.remove(&device);
            }
        }
        requests
    }
}

fn error(message: &str) -> Vec<u8> {
    let message: String = message.chars().take(200).collect();
    json!({ "type": "error", "message": message })
        .to_string()
        .into_bytes()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
