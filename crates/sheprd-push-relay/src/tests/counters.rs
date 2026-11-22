//! The counters: each outcome moves its own, devices are counted without
//! being named, and nothing a caller sent reaches `/metrics`.

use super::{
    attest::{PRODUCTION, Pki},
    attest_relay::attest_request,
    *,
};
use crate::{
    devices::DAY_S,
    metrics::Metrics,
    relay::{AttestRequest, Reply, RevokeRequest},
};
use sheprd_dashboard::metrics::Login;

/// The value of the series `line` names (`family{label="value"}`).
fn value(metrics: &Metrics, now_s: u64, series: &str) -> u64 {
    let text = metrics.render(now_s);
    text.lines()
        .find_map(|line| line.strip_prefix(series)?.strip_prefix(' '))
        .unwrap_or_else(|| panic!("no {series}\n{text}"))
        .parse()
        .unwrap()
}

fn pushes(metrics: &Metrics, outcome: &str) -> u64 {
    value(
        metrics,
        NOW_S,
        &format!("sheprd_push_relay_pushes_total{{outcome=\"{outcome}\"}}"),
    )
}

#[test]
fn each_reply_has_its_wire_name_as_its_label() {
    for reply in Reply::ALL {
        assert_eq!(
            serde_json::to_string(reply).unwrap(),
            format!("\"{}\"", reply.label())
        );
    }
}

#[tokio::test]
async fn each_push_outcome_moves_its_counter_and_type() {
    let fixture = fixture_with(Rates {
        handle: tight(2),
        ..Rates::default()
    });
    let relay = &fixture.relay;
    let metrics = relay.metrics();
    let send = |handle: String, push_type| relay.push(source(), request(handle, push_type), NOW_MS);

    assert_eq!(
        send(handle(PHONE, "alert", 1), PushType::Alert).await,
        Reply::Delivered
    );
    fixture.apple.answer(Ok(Outcome::Gone));
    assert_eq!(
        send(handle(PHONE, "alert", 2), PushType::Alert).await,
        Reply::Gone
    );
    assert_eq!(
        send("junk".to_owned(), PushType::Background).await,
        Reply::Refused
    );
    fixture.apple.answer(Err(sheprd_apns::Error::Timeout));
    let activity = handle(OTHER_PHONE, "activity", 3);
    assert_eq!(
        send(activity, PushType::LiveActivity).await,
        Reply::Unavailable
    );
    // The handle limit, then Apple's own.
    assert_eq!(
        send(handle(PHONE, "alert", 1), PushType::Alert).await,
        Reply::Delivered
    );
    assert_eq!(
        send(handle(PHONE, "alert", 1), PushType::Alert).await,
        Reply::Limited
    );
    fixture.apple.answer(Ok(Outcome::RateLimited));
    assert_eq!(
        send(handle(PHONE, "alert", 4), PushType::Alert).await,
        Reply::Limited
    );

    for (outcome, count) in [
        ("delivered", 2),
        ("gone", 1),
        ("refused", 1),
        ("unavailable", 1),
        ("limited", 2),
        ("expired", 0),
    ] {
        assert_eq!(pushes(metrics, outcome), count, "{outcome}");
    }
    for (kind, count) in [("alert", 5), ("background", 1), ("liveactivity", 1)] {
        let series = format!("sheprd_push_relay_push_types_total{{type=\"{kind}\"}}");
        assert_eq!(value(metrics, NOW_S, &series), count, "{kind}");
    }
    for (limit, count) in [("handle", 1), ("apns", 1), ("source", 0)] {
        let series = format!("sheprd_push_relay_limited_total{{limit=\"{limit}\"}}");
        assert_eq!(value(metrics, NOW_S, &series), count, "{limit}");
    }
}

#[tokio::test]
async fn devices_that_push_are_counted_for_a_day_and_a_week() {
    let fixture = fixture();
    for (device, id) in [(PHONE, 1), (PHONE, 2), (OTHER_PHONE, 3)] {
        let push = request(handle(device, "alert", id), PushType::Alert);
        fixture.relay.push(source(), push, NOW_MS).await;
    }
    // A refused push names no device.
    fixture
        .relay
        .push(
            source(),
            request("junk".to_owned(), PushType::Alert),
            NOW_MS,
        )
        .await;
    let counts = |now_s| fixture.relay.metrics().totals(now_s).devices;
    assert_eq!(
        (counts(NOW_S).active_day, counts(NOW_S).active_week),
        (2, 2)
    );
    let later = NOW_S + 2 * DAY_S;
    assert_eq!(
        (counts(later).active_day, counts(later).active_week),
        (0, 2)
    );
    assert_eq!(counts(NOW_S + 8 * DAY_S).active_week, 0);
    let metrics = fixture.relay.metrics();
    let active = "sheprd_push_relay_active_devices{window=\"24h\"}";
    assert_eq!(value(metrics, NOW_S, active), 2);
}

#[test]
fn attestations_and_challenges_are_counted_by_outcome() {
    let pki = Pki::new();
    let fixture = fixture_attesting(
        Rates {
            attest: tight(3),
            ..Rates::default()
        },
        Host::Production,
        pki.verifier(),
    );
    let relay = &fixture.relay;
    let challenge = relay.challenge(source(), NOW_MS).unwrap();
    let (attested, _) = attest_request(&pki, &challenge, PRODUCTION);
    relay.attest(source(), &attested, NOW_MS).unwrap();
    let junk = AttestRequest {
        attestation: "!".to_owned(),
        challenge,
        key_id: "!".to_owned(),
    };
    assert_eq!(relay.attest(source(), &junk, NOW_MS), Err(Reply::Invalid));
    assert_eq!(relay.challenge(source(), NOW_MS), Err(Reply::Limited));

    let metrics = relay.metrics();
    for (series, count) in [
        (
            "sheprd_push_relay_attestations_total{outcome=\"issued\"}",
            1,
        ),
        (
            "sheprd_push_relay_attestations_total{outcome=\"invalid\"}",
            1,
        ),
        ("sheprd_push_relay_challenges_total{outcome=\"issued\"}", 1),
        ("sheprd_push_relay_challenges_total{outcome=\"limited\"}", 1),
        ("sheprd_push_relay_limited_total{limit=\"attest\"}", 1),
        ("sheprd_push_relay_attested_devices", 1),
    ] {
        assert_eq!(value(metrics, NOW_S, series), count, "{series}");
    }
}

#[tokio::test]
async fn revocations_are_counted_by_reply() {
    let fixture = fixture_with(Rates {
        revoke: tight(1),
        ..Rates::default()
    });
    let revoke = |handle: String| RevokeRequest {
        handle: handle.into(),
    };
    let relay = &fixture.relay;
    let first = revoke(handle(PHONE, "alert", 1));
    assert_eq!(relay.revoke(source(), first, NOW_MS).await, Reply::Revoked);
    let second = revoke(handle(PHONE, "alert", 2));
    assert_eq!(relay.revoke(source(), second, NOW_MS).await, Reply::Limited);
    let metrics = relay.metrics();
    for (series, count) in [
        (
            "sheprd_push_relay_revocations_total{outcome=\"revoked\"}",
            1,
        ),
        (
            "sheprd_push_relay_revocations_total{outcome=\"limited\"}",
            1,
        ),
        ("sheprd_push_relay_limited_total{limit=\"revoke\"}", 1),
    ] {
        assert_eq!(value(metrics, NOW_S, series), count, "{series}");
    }
}

#[tokio::test]
async fn nothing_a_caller_sent_reaches_the_metrics() {
    let fixture = fixture();
    let handle = handle(PHONE, "alert", 7);
    let push = request(handle.clone(), PushType::Alert);
    fixture.relay.push(source(), push, NOW_MS).await;
    let metrics = fixture.relay.metrics();
    metrics.login(Login::Failed);
    let text = metrics.render(NOW_S);
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let device = hex(&PHONE);
    let sealed = handle.split('.').nth(1).unwrap();
    for needle in [
        token_hex(PHONE[0]).as_str(),
        handle.as_str(),
        sealed,
        &"07".repeat(16),
        &device,
        &source().to_string(),
        "sealed",
        "bm90aGluZyB0byBzZWU",
    ] {
        assert!(!text.contains(needle), "/metrics holds {needle}\n{text}");
    }
    // Every series is a fixed family with fixed labels: a name, then at
    // most one label from a closed set, then a number.
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let (series, number) = line.rsplit_once(' ').unwrap();
        assert!(number.parse::<f64>().is_ok(), "{line}");
        assert!(series.starts_with("sheprd_push_relay_"), "{line}");
        let label = series.split_once('{').map(|(_, label)| label);
        if let Some(label) = label {
            let value = label.split('"').nth(1).unwrap();
            assert!(
                value.len() <= 16
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._".contains(&byte)),
                "{line}"
            );
        }
    }
    let json = serde_json::to_string(&metrics.totals(NOW_S)).unwrap();
    assert!(!json.contains(&device) && !json.contains(&token_hex(PHONE[0])));
}
