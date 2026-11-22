//! Activities go to the push relay's handles. A plain token, which an older
//! app registers, is never sent to and nothing waits for one; with the relay
//! off nothing goes at all.

use super::*;
use serde_json::json;

fn relay() -> Delivery {
    Delivery::Relay(Box::new(crate::apns::Relays::new().unwrap()))
}

fn handle(byte: char) -> serde_json::Value {
    json!({"handle": format!("1a2b3c4d.{}", byte.to_string().repeat(160)), "environment": "sandbox"})
}

#[test]
fn the_relay_sends_to_handles() {
    let relay = relay();
    let registered = registration(
        &json!({
            "push_to_start": handle('A'),
            "activities": [{"pane": "w1-p3", "token": handle('B')}],
        })
        .to_string(),
    );
    let end = live(Event::End { dismissal_s: 0 });
    let Route::Send(Recipient::Handle(sent)) = route(&end, &registered, &relay) else {
        panic!("expected a send to a handle");
    };
    assert_eq!(
        sent.secret().expose_secret(),
        format!("1a2b3c4d.{}", "B".repeat(160))
    );

    // Without the activity's handle yet, an end or update waits for it.
    let start_only = registration(&json!({"push_to_start": handle('A')}).to_string());
    assert_eq!(route(&end, &start_only, &relay), Route::Hold);
    let update = live(Event::Update { alert: None });
    assert_eq!(route(&update, &start_only, &relay), Route::Hold);

    // A device that cannot start activities shows none to update.
    assert_eq!(route(&end, &Registration::default(), &relay), Route::Skip);
}

#[test]
fn plain_tokens_are_never_sent_to() {
    let tokens = registration(
        &json!({
            "push_to_start": "ab".repeat(32),
            "activities": [{"pane": "w1-p3", "token": "cd".repeat(40)}],
        })
        .to_string(),
    );
    let end = live(Event::End { dismissal_s: 0 });
    assert_eq!(route(&end, &tokens, &relay()), Route::Skip);
    assert_eq!(route(&end, &tokens, &Delivery::Off), Route::Skip);
    // With the relay off, not even a handle is sent to.
    let handles = registration(
        &json!({
            "push_to_start": handle('A'),
            "activities": [{"pane": "w1-p3", "token": handle('B')}],
        })
        .to_string(),
    );
    assert_eq!(route(&end, &handles, &Delivery::Off), Route::Skip);
}
