use super::*;
use std::collections::BTreeMap;

fn info(relays: usize) -> Info {
    Info {
        identity: "aWRlbnRpdHk".into(),
        labels: BTreeMap::new(),
        relays: (0..relays)
            .map(|index| format!("https://relay{index}.example.org"))
            .collect(),
        route: "cm91dGU".into(),
        rtt_ms: BTreeMap::new(),
    }
}

fn link() -> PairingLink {
    PairingLink {
        computer: "Studio".into(),
        fingerprint: [7; 32],
        fleet: None,
        relay: info(2),
        token: Token([9; 32]),
    }
}

/// `link` with its JSON changed by `edit`, written as a link again.
fn edited(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let text = link().to_string();
    let json = URL_SAFE_NO_PAD
        .decode(text.strip_prefix(PREFIX).unwrap())
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    edit(&mut value);
    format!(
        "{PREFIX}{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
    )
}

#[test]
fn a_link_reads_back_as_written() {
    let written = link();
    let text = written.to_string();
    assert!(text.starts_with(PREFIX));
    assert!(text.len() <= MAX_LEN);
    assert_eq!(text.parse::<PairingLink>().unwrap(), written);
}

#[test]
fn a_link_with_a_fleet_keeps_it_and_drops_one_that_is_no_fleet_id() {
    let mut written = link();
    written.fleet = Some(Hint {
        fleet: "ab".repeat(32),
        computers: 2,
        hosts: 3,
    });
    let read: PairingLink = written.to_string().parse().unwrap();
    assert_eq!(read.fleet, written.fleet);
    let text = edited(|value| {
        value["fleet"] = serde_json::json!({"fleet": "nope", "computers": 1, "hosts": 0})
    });
    assert_eq!(text.parse::<PairingLink>().unwrap().fleet, None);
}

#[test]
fn white_space_a_chat_inserted_is_dropped() {
    let text = link().to_string();
    let (head, tail) = text.split_at(30);
    let pasted = format!("  {head}\n{tail} \n");
    assert_eq!(pasted.parse::<PairingLink>().unwrap(), link());
}

#[test]
fn a_real_tunnel_identity_fits() {
    let mut written = link();
    written.relay.identity = URL_SAFE_NO_PAD.encode([1; 1_216]);
    written.relay.relays = (0..MAX_RELAYS)
        .map(|index| format!("https://relay-{index}.example.org:8443/sheprd"))
        .collect();
    written.computer = "c".repeat(64);
    let text = written.to_string();
    assert!(text.len() <= MAX_LEN, "{} characters", text.len());
    assert_eq!(text.parse::<PairingLink>().unwrap(), written);
}

#[test]
fn other_text_is_not_a_link() {
    for text in [
        "",
        "https://192.168.1.20:7443/pair#t=abc&fp=def",
        "sheprd-pair:",
        "sheprd-pair:!!!",
        "sheprd-pair:e30",
    ] {
        assert!(
            matches!(text.parse::<PairingLink>(), Err(Error::Format)),
            "{text:?}"
        );
    }
}

#[test]
fn a_bad_field_is_refused() {
    let edits: [fn(&mut serde_json::Value); 6] = [
        |value| value["token"] = "short".into(),
        |value| value["fingerprint"] = "c2hvcnQ".into(),
        |value| value["computer"] = "".into(),
        |value| value["computer"] = "a\u{7}b".into(),
        |value| value["computer"] = "c".repeat(65).into(),
        |value| value["relay"]["route"] = "".into(),
    ];
    for edit in edits {
        assert!(matches!(
            edited(edit).parse::<PairingLink>(),
            Err(Error::Format)
        ));
    }
}

#[test]
fn a_link_naming_no_relay_says_so() {
    let text = edited(|value| value["relay"]["relays"] = serde_json::json!([]));
    assert!(matches!(text.parse::<PairingLink>(), Err(Error::NoRelays)));
}

#[test]
fn a_link_naming_too_many_relays_is_refused() {
    let mut written = link();
    written.relay = info(MAX_RELAYS + 1);
    assert!(matches!(
        written.to_string().parse::<PairingLink>(),
        Err(Error::Format)
    ));
}

#[test]
fn a_newer_version_asks_for_an_update() {
    let text = edited(|value| value["v"] = 2.into());
    assert!(matches!(text.parse::<PairingLink>(), Err(Error::Version)));
}

#[test]
fn an_oversized_paste_is_refused_before_it_is_read() {
    let text = format!("{PREFIX}{}", "a".repeat(MAX_LEN));
    assert!(matches!(text.parse::<PairingLink>(), Err(Error::TooLong)));
    let huge = " ".repeat(MAX_LEN * 2 + 1);
    assert!(matches!(huge.parse::<PairingLink>(), Err(Error::TooLong)));
}

#[test]
fn debug_never_shows_the_token() {
    let shown = format!("{:?}", link());
    assert!(shown.contains("Token(..)"));
    assert!(!shown.contains("[9, 9"));
}
