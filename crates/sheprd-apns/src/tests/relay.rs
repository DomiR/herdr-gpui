//! A relay that restarts, or fails for a moment, costs a daemon nothing.

use crate::{
    Error, Priority, Push, PushType, SealedPush,
    relay::{RelayClient, RelayOutcome},
    seal::INFO,
};
use http_body_util::Full;
use hyper::{Response, StatusCode, body::Bytes, server::conn::http2, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{net::TcpListener, task::JoinHandle};

fn handle() -> secrecy::SecretString {
    "h".into()
}

/// The cover an alert shows, which travels in the clear.
const CLEAR: [&str; 2] = ["Sheprd", "chat-1"];

fn device() -> p256::PublicKey {
    p256::SecretKey::from_slice(&[5; 32]).unwrap().public_key()
}

/// A push whose body `body` makes from the sealed text, in base64url.
fn built(body: impl FnOnce(String) -> Vec<u8>) -> Result<SealedPush, Error> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    SealedPush::build(&device(), INFO, &CLEAR, |sealer| {
        Ok::<_, Error>(Push {
            body: body(URL_SAFE_NO_PAD.encode(sealer.seal(b"hello")?)),
            collapse_id: Some("chat-1".to_owned()),
            priority: Priority::High,
            push_type: PushType::Alert,
        })
    })
}

fn push() -> SealedPush {
    built(|sealed| {
        format!(r#"{{"aps":{{"alert":{{"title":"Sheprd"}}}},"sealed":"{sealed}"}}"#).into_bytes()
    })
    .unwrap()
}

/// A server that answers `statuses` in turn, then the last for good, and
/// counts what it was sent.
async fn serve(
    listener: TcpListener,
    statuses: Vec<StatusCode>,
    calls: Arc<AtomicUsize>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let statuses = statuses.clone();
            let calls = calls.clone();
            connections.spawn(async move {
                let service = service_fn(move |_| {
                    let at = calls.fetch_add(1, Ordering::SeqCst);
                    let status = statuses[at.min(statuses.len() - 1)];
                    async move {
                        Response::builder()
                            .status(status)
                            .body(Full::new(Bytes::new()))
                    }
                });
                let _ = http2::Builder::new(TokioExecutor::new())
                    .serve_connection(TokioIo::new(tcp), service)
                    .await;
            });
        }
    })
}

async fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

async fn relay(statuses: Vec<StatusCode>) -> (RelayClient, Arc<AtomicUsize>, JoinHandle<()>) {
    let (listener, address) = listener().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let server = serve(listener, statuses, calls.clone()).await;
    (RelayClient::plain(address), calls, server)
}

#[tokio::test]
async fn a_good_push_is_sent_once() {
    let (client, calls, _server) = relay(vec![StatusCode::OK]).await;
    assert_eq!(
        client.post(&handle(), &push()).await,
        RelayOutcome::Delivered
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_relay_that_fails_for_a_moment_gets_the_push_on_a_retry() {
    let (client, calls, _server) = relay(vec![
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::BAD_GATEWAY,
        StatusCode::OK,
    ])
    .await;
    assert_eq!(
        client.post(&handle(), &push()).await,
        RelayOutcome::Delivered
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn a_relay_that_stays_down_ends_in_unavailable() {
    let (client, calls, _server) = relay(vec![StatusCode::SERVICE_UNAVAILABLE]).await;
    assert_eq!(
        client.post(&handle(), &push()).await,
        RelayOutcome::Unavailable
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn what_a_retry_cannot_change_is_tried_once() {
    for (status, outcome) in [
        (StatusCode::GONE, RelayOutcome::Gone),
        (StatusCode::UNAUTHORIZED, RelayOutcome::Stale),
        (StatusCode::FORBIDDEN, RelayOutcome::Stale),
        (StatusCode::TOO_MANY_REQUESTS, RelayOutcome::Limited),
        (StatusCode::BAD_REQUEST, RelayOutcome::Invalid),
    ] {
        let (client, calls, _server) = relay(vec![status]).await;
        assert_eq!(client.post(&handle(), &push()).await, outcome, "{status}");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{status}");
    }
}

#[tokio::test]
async fn a_restarted_relay_is_reached_again_with_nothing_set_up() {
    let (first, address) = listener().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let server = serve(first, vec![StatusCode::OK], calls.clone()).await;
    let client = RelayClient::plain(address);
    assert_eq!(
        client.post(&handle(), &push()).await,
        RelayOutcome::Delivered
    );

    // Everything it held goes with it, its connections included.
    server.abort();
    let _ = server.await;

    let again = TcpListener::bind(address).await.unwrap();
    let _server = serve(again, vec![StatusCode::OK], Arc::default()).await;
    assert_eq!(
        client.post(&handle(), &push()).await,
        RelayOutcome::Delivered
    );
}

#[test]
fn a_payload_that_is_not_json_cannot_be_built() {
    assert!(matches!(built(|_| vec![0xff]), Err(Error::NotSealed)));
}

#[test]
fn the_relays_are_ours_alone() {
    use crate::RelayHost;
    assert_eq!(RelayHost::Production.name(), "push.sheprd.org");
    assert_eq!(RelayHost::Sandbox.name(), "push-sandbox.sheprd.org");
}

#[test]
fn a_push_that_sealed_nothing_cannot_be_built() {
    let built = SealedPush::build(&device(), INFO, &CLEAR, |_| {
        Ok::<_, Error>(Push {
            body: br#"{"aps":{"alert":{"title":"Sheprd"}}}"#.to_vec(),
            collapse_id: None,
            priority: Priority::High,
            push_type: PushType::Alert,
        })
    });
    assert!(matches!(built, Err(Error::NotSealed)));
}

#[test]
fn plain_text_beside_the_sealed_text_cannot_be_built() {
    // Sealed, then dropped for the plain text.
    let discarded = built(|_| br#"{"aps":{"alert":{"title":"Run rm -rf?"}}}"#.to_vec());
    assert!(matches!(discarded, Err(Error::NotSealed)));
    // Sealed, but with plain text beside it.
    let beside = built(|sealed| {
        format!(r#"{{"aps":{{"alert":{{"body":"Run rm -rf?"}}}},"sealed":"{sealed}"}}"#)
            .into_bytes()
    });
    assert!(matches!(beside, Err(Error::NotSealed)));
    // Hidden in an array.
    let listed =
        built(|sealed| format!(r#"{{"sealed":"{sealed}","more":["Run rm -rf?"]}}"#).into_bytes());
    assert!(matches!(listed, Err(Error::NotSealed)));
}

#[test]
fn a_collapse_id_must_be_named_clear() {
    let built = SealedPush::build(&device(), INFO, &CLEAR, |sealer| {
        let sealed = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            sealer.seal(b"hello")?,
        );
        Ok::<_, Error>(Push {
            body: format!(r#"{{"sealed":"{sealed}"}}"#).into_bytes(),
            collapse_id: Some("remote plan".to_owned()),
            priority: Priority::High,
            push_type: PushType::Alert,
        })
    });
    assert!(matches!(built, Err(Error::NotSealed)));
}

#[test]
fn what_is_sealed_opens_with_the_phones_key() {
    let key = p256::SecretKey::from_slice(&[5; 32]).unwrap();
    let push = push();
    let body: serde_json::Value = serde_json::from_slice(&push.push().body).unwrap();
    let sealed = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        body["sealed"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(crate::seal::open(&key, &sealed, INFO).unwrap(), b"hello");
}

#[test]
fn a_handles_environment_names_its_relay() {
    use crate::RelayHost;
    let read: Vec<RelayHost> = serde_json::from_str(r#"["production","sandbox"]"#).unwrap();
    assert_eq!(read, [RelayHost::Production, RelayHost::Sandbox]);
    assert!(serde_json::from_str::<RelayHost>(r#""push.evil.example""#).is_err());
}
