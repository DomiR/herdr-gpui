//! The scanning side without a radio: what Sheprd hears and what the
//! radio is asked, as `macos/sheprd-nearby/Central.swift` behaves.

use crate::{
    Error,
    central::{self, Action, Central, Command, Inbox, Input, Link, Step},
    framing::{MIN_CHUNK, chunks},
    protocol::{Event, Out, Radio},
};
use std::sync::{Arc, Mutex, mpsc};

mod races;

const DEVICE: &str = "/org/bluez/hci0/dev_AA";

fn info(name: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "v": 1, "name": name, "fp": "ABCD" })).unwrap()
}

fn act(action: Action) -> Step {
    Step::Act(action)
}

fn emit(event: Event) -> Step {
    Step::Emit(event)
}

/// A central that scans, with the radio on.
fn scanning() -> Central {
    let mut central = Central::new(7);
    central.input(Input::Radio(Radio::PoweredOn));
    central.command(Command::Scan(true));
    central
}

/// A computer found, connected, resolved, and listed under `name`.
fn listed(central: &mut Central, name: &str) -> String {
    central.input(Input::Discovered(DEVICE.into()));
    central.input(Input::Connected(DEVICE.into()));
    central.input(Input::Resolved {
        device: DEVICE.into(),
        info: true,
        pair: true,
        chunk: 100,
    });
    central.input(Input::Info {
        device: DEVICE.into(),
        attempt: 1,
        value: Some(info(name)),
    });
    central.id(DEVICE)
}

#[test]
fn scanning_discovers_only_with_the_radio_on() {
    let mut central = Central::new(1);
    assert_eq!(central.command(Command::Scan(true)), []);
    assert_eq!(
        central.input(Input::Radio(Radio::PoweredOn)),
        [
            emit(Event::CentralState {
                state: Radio::PoweredOn
            }),
            act(Action::Discover(true)),
        ]
    );
    // A page opened again learns the radio at once.
    central.command(Command::Scan(false));
    assert_eq!(
        central.command(Command::Scan(true)),
        [
            emit(Event::CentralState {
                state: Radio::PoweredOn
            }),
            act(Action::Discover(true)),
        ]
    );
    // The radio going off stops discovery; coming back resumes it.
    central.input(Input::Radio(Radio::PoweredOff));
    assert_eq!(
        central.input(Input::Radio(Radio::PoweredOn)),
        [
            emit(Event::CentralState {
                state: Radio::PoweredOn
            }),
            act(Action::Discover(true)),
        ]
    );
}

#[test]
fn a_computer_is_connected_resolved_read_and_listed_once() {
    let mut central = scanning();
    assert_eq!(
        central.input(Input::Discovered(DEVICE.into())),
        [act(Action::Connect(DEVICE.into()))]
    );
    // Seen again, it is not connected again.
    assert_eq!(central.input(Input::Discovered(DEVICE.into())), []);
    assert_eq!(
        central.input(Input::Connected(DEVICE.into())),
        [act(Action::Resolve(DEVICE.into()))]
    );
    assert_eq!(
        central.input(Input::Resolved {
            device: DEVICE.into(),
            info: true,
            pair: true,
            chunk: 180,
        }),
        [act(Action::ReadInfo {
            device: DEVICE.into(),
            attempt: 1,
        })]
    );
    let id = central.id(DEVICE);
    assert_eq!(
        central.input(Input::Info {
            device: DEVICE.into(),
            attempt: 1,
            value: Some(info("Studio")),
        }),
        [emit(Event::Found {
            peripheral: id.clone(),
            name: "Studio".into(),
            fp: "ABCD".into(),
        })]
    );
    assert_eq!(
        central.input(Input::Info {
            device: DEVICE.into(),
            attempt: 1,
            value: Some(info("Studio")),
        }),
        []
    );
    // Ids are what the app accepts: shaped like Core Bluetooth's UUIDs.
    assert_eq!(id.len(), 36);
}

#[test]
fn a_computer_not_pairing_is_not_listed_and_one_that_stops_is_unlisted() {
    let mut central = scanning();
    central.input(Input::Discovered(DEVICE.into()));
    central.input(Input::Connected(DEVICE.into()));
    central.input(Input::Resolved {
        device: DEVICE.into(),
        info: true,
        pair: true,
        chunk: 20,
    });
    // Presence alone gives no name.
    assert_eq!(
        central.input(Input::Info {
            device: DEVICE.into(),
            attempt: 1,
            value: Some(br#"{"v":1}"#.to_vec()),
        }),
        []
    );
    let id = listed(&mut central, "Studio");
    // The dialog closed: the read is refused.
    assert_eq!(
        central.input(Input::Info {
            device: DEVICE.into(),
            attempt: 1,
            value: None,
        }),
        [emit(Event::PeripheralDisconnected { peripheral: id })]
    );
}

#[test]
fn names_are_cleaned_and_bounded() {
    let found = |name: &str| {
        let mut central = scanning();
        central.input(Input::Discovered(DEVICE.into()));
        central.input(Input::Connected(DEVICE.into()));
        central.input(Input::Resolved {
            device: DEVICE.into(),
            info: true,
            pair: false,
            chunk: 20,
        });
        central.input(Input::Info {
            device: DEVICE.into(),
            attempt: 1,
            value: Some(info(name)),
        })
    };
    let steps = found(&format!("Stu\u{7}dio{}", "x".repeat(100)));
    let [Step::Emit(Event::Found { name, .. })] = &steps[..] else {
        panic!("not found: {steps:?}");
    };
    assert_eq!(name.chars().count(), 64);
    assert!(name.starts_with("Studio"));
    // A blank name is no name.
    assert_eq!(found("  "), []);
}

#[test]
fn a_device_without_the_service_is_let_go() {
    let mut central = scanning();
    central.input(Input::Discovered(DEVICE.into()));
    central.input(Input::Connected(DEVICE.into()));
    assert_eq!(
        central.input(Input::Resolved {
            device: DEVICE.into(),
            info: false,
            pair: false,
            chunk: 20,
        }),
        [act(Action::Disconnect(DEVICE.into()))]
    );
    // Gone, it may be found again.
    assert_eq!(
        central.input(Input::Discovered(DEVICE.into())),
        [act(Action::Connect(DEVICE.into()))]
    );
}

#[test]
fn connecting_subscribes_and_reports_ready() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    assert_eq!(
        central.command(Command::Connect(id.clone())),
        [act(Action::Subscribe {
            device: DEVICE.into(),
            on: true
        })]
    );
    assert_eq!(
        central.input(Input::Subscribed {
            device: DEVICE.into(),
            notifying: true,
        }),
        [emit(Event::Connected {
            peripheral: id.clone()
        })]
    );
    // A computer being paired is not read again while scanning.
    assert_eq!(central.tick(), []);
    // A failed subscription is a disconnection.
    assert_eq!(
        central.input(Input::Subscribed {
            device: DEVICE.into(),
            notifying: false,
        }),
        [emit(Event::PeripheralDisconnected { peripheral: id })]
    );
}

#[test]
fn connecting_before_resolution_finishes_it_first() {
    let mut central = scanning();
    central.input(Input::Discovered(DEVICE.into()));
    let id = central.id(DEVICE);
    assert_eq!(
        central.command(Command::Connect(id)),
        [act(Action::Connect(DEVICE.into()))]
    );
    central.input(Input::Connected(DEVICE.into()));
    assert_eq!(
        central.input(Input::Resolved {
            device: DEVICE.into(),
            info: true,
            pair: true,
            chunk: 20,
        }),
        [
            act(Action::ReadInfo {
                device: DEVICE.into(),
                attempt: 1,
            }),
            act(Action::Subscribe {
                device: DEVICE.into(),
                on: true
            }),
        ]
    );
}

#[test]
fn connecting_to_an_unknown_computer_reports_it_gone() {
    let mut central = scanning();
    assert_eq!(
        central.command(Command::Connect("NOPE".into())),
        [emit(Event::PeripheralDisconnected {
            peripheral: "NOPE".into()
        })]
    );
}

#[test]
fn frames_are_written_one_acknowledged_chunk_at_a_time() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    central.command(Command::Connect(id.clone()));
    let message = vec![7; 250];
    let expected = chunks(&message, 100);
    assert_eq!(expected.len(), 3);
    assert_eq!(
        central.command(Command::Write {
            peripheral: id.clone(),
            data: message,
        }),
        [act(Action::Write {
            device: DEVICE.into(),
            chunk: expected[0].clone()
        })]
    );
    // A second message queues behind the first.
    assert_eq!(
        central.command(Command::Write {
            peripheral: id,
            data: vec![1],
        }),
        []
    );
    let written = |central: &mut Central| {
        central.input(Input::Written {
            device: DEVICE.into(),
            failure: None,
        })
    };
    assert_eq!(
        written(&mut central),
        [act(Action::Write {
            device: DEVICE.into(),
            chunk: expected[1].clone()
        })]
    );
    assert_eq!(
        written(&mut central),
        [act(Action::Write {
            device: DEVICE.into(),
            chunk: expected[2].clone()
        })]
    );
    assert_eq!(
        written(&mut central),
        [act(Action::Write {
            device: DEVICE.into(),
            chunk: vec![0, 1, 1]
        })]
    );
    assert_eq!(written(&mut central), []);
}

#[test]
fn a_failed_write_drops_the_rest_and_is_logged() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    central.command(Command::Write {
        peripheral: id,
        data: vec![7; 250],
    });
    let steps = central.input(Input::Written {
        device: DEVICE.into(),
        failure: Some("timed out".into()),
    });
    assert!(
        matches!(&steps[..], [Step::Emit(Event::Error { message })] if message == "write: timed out")
    );
    assert_eq!(
        central.input(Input::Written {
            device: DEVICE.into(),
            failure: None,
        }),
        []
    );
}

#[test]
fn a_write_before_pair_is_known_goes_nowhere() {
    let mut central = scanning();
    central.input(Input::Discovered(DEVICE.into()));
    let id = central.id(DEVICE);
    assert_eq!(
        central.command(Command::Write {
            peripheral: id,
            data: vec![1],
        }),
        []
    );
    assert_eq!(
        central.command(Command::Write {
            peripheral: "NOPE".into(),
            data: vec![1],
        }),
        []
    );
}

#[test]
fn notifications_become_whole_frames() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    let message = br#"{"type":"offer"}"#;
    let mut steps = Vec::new();
    for chunk in chunks(message, MIN_CHUNK) {
        steps.extend(central.input(Input::Notified {
            device: DEVICE.into(),
            value: chunk,
        }));
    }
    assert_eq!(steps, [emit(Event::peripheral_frame(&id, message))]);
    // An invalid frame is dropped and the next one still reads.
    assert_eq!(
        central.input(Input::Notified {
            device: DEVICE.into(),
            value: vec![0, 1, 2, 3],
        }),
        []
    );
    let steps: Vec<Step> = chunks(b"ok", MIN_CHUNK)
        .into_iter()
        .flat_map(|chunk| {
            central.input(Input::Notified {
                device: DEVICE.into(),
                value: chunk,
            })
        })
        .collect();
    assert_eq!(steps, [emit(Event::peripheral_frame(&id, b"ok"))]);
}

#[test]
fn info_is_read_again_while_scanning() {
    let mut central = scanning();
    listed(&mut central, "Studio");
    assert_eq!(
        central.tick(),
        [act(Action::ReadInfo {
            device: DEVICE.into(),
            attempt: 1,
        })]
    );
    central.command(Command::Scan(false));
    assert_eq!(central.tick(), []);
}

#[test]
fn stopping_the_scan_lets_go_of_all_but_the_one_being_paired() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    let other = "/org/bluez/hci0/dev_BB";
    central.input(Input::Discovered(other.into()));
    central.command(Command::Connect(id.clone()));
    let steps = central.command(Command::Scan(false));
    assert_eq!(
        steps,
        [
            act(Action::Discover(false)),
            act(Action::Disconnect(other.into())),
        ]
    );
    // Disconnecting it now, with no scan, closes its link and unlists it.
    assert_eq!(
        central.command(Command::Disconnect(id.clone())),
        [act(Action::Disconnect(DEVICE.into()))]
    );
    assert_eq!(
        central.command(Command::Connect(id.clone())),
        [emit(Event::PeripheralDisconnected { peripheral: id })]
    );
}

#[test]
fn stopping_the_scan_unlists_computers_it_lets_go() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    assert_eq!(
        central.command(Command::Scan(false)),
        [
            act(Action::Discover(false)),
            act(Action::Disconnect(DEVICE.into())),
            emit(Event::PeripheralDisconnected { peripheral: id }),
        ]
    );
}

#[test]
fn disconnecting_while_scanning_unsubscribes_and_keeps_it_listed() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    central.command(Command::Connect(id.clone()));
    assert_eq!(
        central.command(Command::Disconnect(id)),
        [act(Action::Subscribe {
            device: DEVICE.into(),
            on: false
        })]
    );
    // Its unsubscription is no news, and it is read again.
    assert_eq!(
        central.input(Input::Subscribed {
            device: DEVICE.into(),
            notifying: false,
        }),
        []
    );
    assert_eq!(
        central.tick(),
        [act(Action::ReadInfo {
            device: DEVICE.into(),
            attempt: 1,
        })]
    );
}

#[test]
fn a_lost_link_is_reported_once_for_a_listed_computer() {
    let mut central = scanning();
    let id = listed(&mut central, "Studio");
    assert_eq!(
        central.input(Input::Disconnected(DEVICE.into())),
        [emit(Event::PeripheralDisconnected { peripheral: id })]
    );
    assert_eq!(central.input(Input::Disconnected(DEVICE.into())), []);
    // Unknown devices, such as phones on the peripheral side, are ignored.
    assert_eq!(central.input(Input::Disconnected("/other".into())), []);
}

/// Records what the loop asks of the radio.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<Action>>>);

impl Link for Recorder {
    fn act(&self, action: Action) {
        self.0.lock().unwrap().push(action);
    }
}

#[test]
fn the_loop_opens_the_radio_at_the_first_scan_and_applies_in_order() {
    let (sender, messages) = mpsc::channel();
    let inbox = Inbox::new(sender);
    let (sink, events) = mpsc::channel();
    let recorder = Recorder::default();
    let opened = Arc::new(Mutex::new(0));
    // Pairing's commands never open the radio.
    inbox.command(Command::Disconnect("NOPE".into()));
    inbox.command(Command::Scan(true));
    inbox.send(Input::Discovered(DEVICE.into()));
    inbox.command(Command::Scan(false));
    let (link, count) = (recorder.clone(), opened.clone());
    let reporter = inbox.clone();
    drop(inbox);
    // The loop holds a sender of its own, as the platform's callbacks do,
    // so it runs on after the test; it only waits.
    std::thread::spawn(move || {
        central::run(&messages, &reporter, &Out::to(sink), |inbox: &Inbox| {
            *count.lock().unwrap() += 1;
            // The platform reports its radio as it opens: after what was
            // already queued.
            inbox.send(Input::Radio(Radio::PoweredOn));
            Ok(link.clone())
        });
    });
    assert_eq!(
        events.recv().unwrap(),
        Event::CentralState {
            state: Radio::PoweredOn
        }
    );
    assert_eq!(*opened.lock().unwrap(), 1);
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            Action::Connect(DEVICE.into()),
            Action::Disconnect(DEVICE.into()),
        ]
    );
}

#[test]
fn a_radio_that_cannot_open_is_reported_unsupported_and_tried_again() {
    let (sender, messages) = mpsc::channel();
    let inbox = Inbox::new(sender);
    let (sink, events) = mpsc::channel();
    inbox.command(Command::Scan(true));
    inbox.command(Command::Scan(true));
    let reporter = inbox.clone();
    drop(inbox);
    let tries = Arc::new(Mutex::new(0));
    let count = tries.clone();
    std::thread::spawn(move || {
        central::run(&messages, &reporter, &Out::to(sink), |_: &Inbox| {
            *count.lock().unwrap() += 1;
            Err::<Recorder, _>(Error::Gatt {
                step: "find a Bluetooth adapter",
                code: 0,
            })
        });
    });
    let mut seen = Vec::new();
    while seen.len() < 4 {
        seen.push(events.recv().unwrap());
    }
    assert!(matches!(&seen[0], Event::Error { message } if message.starts_with("scanning: ")));
    assert_eq!(
        seen[1],
        Event::CentralState {
            state: Radio::Unsupported
        }
    );
    assert!(matches!(&seen[2], Event::Error { .. }));
    assert_eq!(*tries.lock().unwrap(), 2);
}
