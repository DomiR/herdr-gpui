//! A registration's tokens may be relay handles, and what older daemons
//! stored keeps loading.

use super::*;
use secrecy::ExposeSecret as _;
use serde_json::json;
use sheprd_apns::RelayHost;

const KEY_ID: &str = "1a2b3c4d";

fn sealed(len: usize) -> String {
    "A".repeat(len)
}

fn handle_text(byte: char) -> String {
    format!("{KEY_ID}.{}", byte.to_string().repeat(160))
}

fn handle(byte: char) -> Recipient {
    Handle::parse(&handle_text(byte), RelayHost::Production)
        .unwrap()
        .into()
}

#[test]
fn what_an_older_daemon_stored_still_loads() {
    let stored = json!({
        "push_to_start": "ab".repeat(32),
        "alert": "9f".repeat(32),
        "activities": [{ "pane": "w1-p3", "token": "cd".repeat(40) }],
    });
    let registration = Registration::from_stored(Some(&stored));
    assert_eq!(registration.push_to_start, Some(token(0xab)));
    assert_eq!(registration.alert, Some(token(0x9f)));
    assert!(matches!(
        registration.activity("w1-p3"),
        Some(Recipient::Token(_))
    ));
    // And is stored again in the same shape.
    assert_eq!(registration.to_stored().unwrap(), Some(stored));
}

#[test]
fn a_phone_on_the_relay_registers_handles() {
    let body = json!({
        "push_to_start": { "handle": handle_text('B'), "environment": "sandbox" },
        "alert": { "handle": handle_text('C'), "environment": "production" },
        "activities": [{ "pane": "w1-p3",
                         "token": { "handle": handle_text('D'), "environment": "sandbox" } }],
    });
    let registration = Registration::parse(body.to_string().as_bytes()).unwrap();
    let Some(Recipient::Handle(start)) = &registration.push_to_start else {
        panic!("expected a handle");
    };
    assert_eq!(start.environment(), RelayHost::Sandbox);
    assert_eq!(start.secret().expose_secret(), handle_text('B'));
    let Some(Recipient::Handle(alert)) = &registration.alert else {
        panic!("expected a handle");
    };
    assert_eq!(alert.environment(), RelayHost::Production);

    let stored = registration.to_stored().unwrap();
    assert_eq!(stored.as_ref(), Some(&body));
    assert_eq!(Registration::from_stored(stored.as_ref()), registration);
}

#[test]
fn a_handle_the_relay_called_gone_is_forgotten() {
    let mut registration = Registration {
        activities: vec![ActivityToken {
            pane: "w1-p3".to_owned(),
            token: handle('E'),
        }],
        alert: Some(handle('F')),
        ..Registration::default()
    };
    assert!(registration.forget(&handle('E')));
    assert!(registration.activities.is_empty());
    assert!(!registration.forget(&handle('E')));
    // The same text for the other relay is another recipient.
    let sandbox = Handle::parse(&handle_text('F'), RelayHost::Sandbox).unwrap();
    assert!(!registration.forget(&sandbox.into()));
    assert!(registration.forget(&handle('F')));
}

#[test]
fn malformed_handles_are_refused() {
    let long = format!("{KEY_ID}.{}", sealed(2048));
    for text in [
        String::new(),
        sealed(200),
        format!("1A2B3C4D.{}", sealed(160)),
        format!("1a2b3c.{}", sealed(160)),
        format!("{KEY_ID}.{}", sealed(100)),
        format!("{KEY_ID}.{}+/", sealed(160)),
        format!("{KEY_ID}.{}=", sealed(160)),
        long,
    ] {
        assert!(
            matches!(
                Handle::parse(&text, RelayHost::Production),
                Err(Error::Handle)
            ),
            "{text}"
        );
    }
    for body in [
        json!({ "alert": { "handle": handle_text('A') } }),
        json!({ "alert": { "handle": handle_text('A'), "environment": "https://evil.example" } }),
        json!({ "alert": { "handle": handle_text('A'), "environment": "sandbox", "host": "x" } }),
        json!({ "alert": "not hex" }),
    ] {
        assert!(
            Registration::parse(body.to_string().as_bytes()).is_err(),
            "{body}"
        );
    }
}

#[test]
fn printing_a_handle_shows_none_of_it() {
    let registration = Registration {
        alert: Some(handle('G')),
        ..Registration::default()
    };
    let printed = format!("{registration:?}");
    assert!(!printed.contains(&"G".repeat(160)), "{printed}");
}
