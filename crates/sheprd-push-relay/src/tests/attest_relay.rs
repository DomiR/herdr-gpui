//! `/v1/challenge` and `/v1/attest`: a phone's first contact with the relay.

use super::{
    attest::{DEVELOPMENT, PRODUCTION, Pki, Shape, attestation},
    *,
};
use crate::{
    relay::{AttestRequest, Reply},
    web::{Source, router},
};
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use base64::engine::general_purpose::STANDARD;
use http_body_util::BodyExt as _;
use std::net::SocketAddr;
use tower::ServiceExt as _;

/// A request for what `pki` attests, over `challenge`, in `aaguid`'s
/// environment.
pub(super) fn attest_request(
    pki: &Pki,
    challenge: &str,
    aaguid: [u8; 16],
) -> (AttestRequest, [u8; 32]) {
    let shape = Shape {
        aaguid,
        challenge: challenge.as_bytes(),
        ..Shape::default()
    };
    let (object, key_id) = attestation(pki, &shape);
    let request = AttestRequest {
        attestation: STANDARD.encode(object),
        challenge: challenge.to_owned(),
        key_id: STANDARD.encode(key_id),
    };
    (request, key_id)
}

#[tokio::test]
async fn an_attested_key_gets_a_credential_and_the_relays_key() {
    let pki = Pki::new();
    let fixture = fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    let challenge = fixture.relay.challenge(source(), NOW_MS).unwrap();
    let (attested, key_id) = attest_request(&pki, &challenge, PRODUCTION);
    let issued = fixture.relay.attest(source(), &attested, NOW_MS).unwrap();

    let credential = issuer().check(&issued.credential).unwrap();
    assert_eq!(credential.device, issuer().device(&key_id));
    assert_eq!(credential.environment, Host::Production);
    assert_eq!(credential.expires_s, issued.expires_s);
    assert_eq!(issued.relay_key_id, key_id_of_relay());
    let point = URL_SAFE_NO_PAD.decode(&issued.relay_key).unwrap();
    assert_eq!(point, relay_key().public_key().to_sec1_bytes().to_vec());

    // A handle made with it pushes.
    let id = "ab".repeat(16);
    let handle = handle_with(&issued.credential, "alert", &id, 3);
    let push = request(handle, PushType::Alert);
    assert_eq!(
        fixture.relay.push(source(), push, NOW_MS).await,
        Reply::Delivered
    );
}

fn key_id_of_relay() -> String {
    key_id(&relay_key())
}

#[test]
fn each_relay_takes_only_its_own_environments_builds() {
    let pki = Pki::new();
    let sandbox = fixture_attesting(Rates::default(), Host::Sandbox, pki.verifier());
    let production = fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    for (fixture, good, bad) in [
        (&sandbox, DEVELOPMENT, PRODUCTION),
        (&production, PRODUCTION, DEVELOPMENT),
    ] {
        let challenge = fixture.relay.challenge(source(), NOW_MS).unwrap();
        let (accepted, _) = attest_request(&pki, &challenge, good);
        assert!(fixture.relay.attest(source(), &accepted, NOW_MS).is_ok());
        let (refused, _) = attest_request(&pki, &challenge, bad);
        assert_eq!(
            fixture.relay.attest(source(), &refused, NOW_MS),
            Err(Reply::Refused)
        );
    }
}

#[test]
fn a_challenge_from_elsewhere_or_too_old_is_refused() {
    let pki = Pki::new();
    let fixture = fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    let forged = Issuer::new(&[6; 32]).unwrap().challenge(NOW_S).unwrap();
    let (request_forged, _) = attest_request(&pki, &forged, PRODUCTION);
    assert_eq!(
        fixture.relay.attest(source(), &request_forged, NOW_MS),
        Err(Reply::Refused)
    );

    let old = fixture.relay.challenge(source(), NOW_MS).unwrap();
    let (late, _) = attest_request(&pki, &old, PRODUCTION);
    let later_ms = NOW_MS + crate::credential::CHALLENGE_LIFETIME_S * 1000;
    assert_eq!(
        fixture.relay.attest(source(), &late, later_ms),
        Err(Reply::Refused)
    );
}

#[test]
fn malformed_requests_are_invalid_and_bad_attestations_refused() {
    let pki = Pki::new();
    let fixture = fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    let challenge = fixture.relay.challenge(source(), NOW_MS).unwrap();
    let (good, _) = attest_request(&pki, &challenge, PRODUCTION);
    let spoil = |change: fn(&mut AttestRequest)| {
        let mut request = AttestRequest {
            attestation: good.attestation.clone(),
            challenge: good.challenge.clone(),
            key_id: good.key_id.clone(),
        };
        change(&mut request);
        fixture.relay.attest(source(), &request, NOW_MS)
    };
    assert_eq!(
        spoil(|r| r.key_id = "not base64!".into()),
        Err(Reply::Invalid)
    );
    assert_eq!(
        spoil(|r| r.key_id = STANDARD.encode([1; 31])),
        Err(Reply::Invalid)
    );
    assert_eq!(
        spoil(|r| r.attestation = "x".repeat(13 * 1024)),
        Err(Reply::Invalid)
    );
    assert_eq!(
        spoil(|r| r.attestation = STANDARD.encode(b"junk")),
        Err(Reply::Refused)
    );
    assert_eq!(
        spoil(|r| r.key_id = STANDARD.encode([1; 32])),
        Err(Reply::Refused)
    );
}

#[test]
fn challenges_and_attestations_share_a_limit_per_source() {
    let pki = Pki::new();
    let rates = Rates {
        attest: tight(2),
        ..Rates::default()
    };
    let fixture = fixture_attesting(rates, Host::Production, pki.verifier());
    let challenge = fixture.relay.challenge(source(), NOW_MS).unwrap();
    let (request, _) = attest_request(&pki, &challenge, PRODUCTION);
    assert!(fixture.relay.attest(source(), &request, NOW_MS).is_ok());
    assert_eq!(
        fixture.relay.attest(source(), &request, NOW_MS),
        Err(Reply::Limited)
    );
    assert_eq!(
        fixture.relay.challenge(source(), NOW_MS),
        Err(Reply::Limited)
    );
    let elsewhere: IpAddr = "198.51.100.7".parse().unwrap();
    assert!(fixture.relay.challenge(elsewhere, NOW_MS).is_ok());
}

fn post(path: &str, body: impl Into<Body>) -> Request<Body> {
    let peer: SocketAddr = "192.0.2.1:4000".parse().unwrap();
    let mut request = Request::post(path)
        .header(CONTENT_TYPE, "application/json")
        .body(body.into())
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn a_phone_attests_over_http() {
    let pki = Pki::new();
    let Fixture { relay, .. } =
        fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    let app = router(relay, Source::Peer, None);

    let (status, body) = send(&app, post("/v1/challenge", "")).await;
    assert_eq!(status, StatusCode::OK);
    let challenge = body["challenge"].as_str().unwrap();
    let (request, _) = attest_request(&pki, challenge, PRODUCTION);
    let json = json!({
        "attestation": request.attestation,
        "challenge": request.challenge,
        "key_id": request.key_id,
    });
    let (status, body) = send(&app, post("/v1/attest", json.to_string())).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        issuer()
            .check(body["credential"].as_str().unwrap())
            .is_some()
    );
    assert_eq!(body["relay_key_id"], key_id_of_relay());

    let (status, body) = send(&app, post("/v1/attest", "junk")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["outcome"], "invalid");
    let unknown = json!({ "attestation": "", "challenge": "", "key_id": "", "extra": 1 });
    let (status, _) = send(&app, post("/v1/attest", unknown.to_string())).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn a_challenge_is_spent_by_the_attestation_it_carries() {
    let pki = Pki::new();
    let fixture = fixture_attesting(Rates::default(), Host::Production, pki.verifier());
    let challenge = fixture.relay.challenge(source(), NOW_MS).unwrap();

    // A refused attestation spends nothing: the phone can still use it.
    let (wrong, _) = attest_request(&pki, &challenge, DEVELOPMENT);
    assert_eq!(
        fixture.relay.attest(source(), &wrong, NOW_MS),
        Err(Reply::Refused)
    );
    let (good, _) = attest_request(&pki, &challenge, PRODUCTION);
    assert!(fixture.relay.attest(source(), &good, NOW_MS).is_ok());

    // Sent again, whole or with another key, it gets nothing.
    assert_eq!(
        fixture.relay.attest(source(), &good, NOW_MS),
        Err(Reply::Refused)
    );
    let (other_key, _) = attest_request(&pki, &challenge, PRODUCTION);
    assert_eq!(
        fixture.relay.attest(source(), &other_key, NOW_MS),
        Err(Reply::Refused)
    );
}
