//! Claiming a nearby Sheprd through its relays: the pairing dial its
//! sealed token opens once, the tunnel's pairing handshake keyed by that
//! token, then its own TLS, pinned to the authority the code covered, and
//! its claim API, on that one connection, until its user answers.

use super::{Error, Keys, Opened, Result, record::Record};
use crate::{
    auth::{self, encode},
    control::fleet::bluetooth::{ApprovedFleet, ClaimFleet},
    ssh_access::{Login, valid_user},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::{Method, Request, body::Bytes, client::conn::http1};
use hyper_util::rt::TokioIo;
use rustls::{
    DigitallySignedStruct, RootCertStore, SignatureScheme,
    client::{
        WebPkiServerVerifier,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    crypto::CryptoProvider,
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sheprd_relay::{Endpoint, RouteId, client, wire::Status};
use std::{
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::io::join;

/// How long the relays take to let the pairing dial through, after the
/// other computer sealed its token.
const DIAL_WAIT: Duration = Duration::from_secs(5);
/// How long a step on the connection may take.
const STEP: Duration = Duration::from_secs(15);
/// How often the claim is asked about while its user decides.
const POLL: Duration = Duration::from_secs(1);
/// The longest wait for an answer: the daemon's two minutes, and some.
const ANSWER: Duration = Duration::from_secs(150);
/// The largest answer read.
const MAX_ANSWER: usize = 64 * 1024;

/// What the claim says about this computer.
pub(crate) struct Claimant<'a> {
    pub(crate) keys: &'a Keys,
    /// This computer's name, as the other computer's dialog shows it.
    pub(crate) name: &'a str,
    /// `ssh-ed25519 BASE64`.
    pub(crate) ssh_key: &'a str,
    /// What the claim asks of the fleet, beside pairing.
    pub(crate) fleet: Option<&'a ClaimFleet>,
}

/// An allowed claim: the machine to keep, and how to log in to it.
#[derive(Debug)]
pub(crate) struct Allowed {
    pub(crate) record: Record,
    pub(crate) login: Login,
    /// What became of the fleet, checked ([`super::joining::checked`]).
    pub(crate) fleet: Option<ApprovedFleet>,
}

/// Claims the computer `opened` describes; `claimed` runs with the code
/// both screens show once its user is asked. Fails unless it is allowed.
pub(crate) async fn claim(
    opened: &Opened,
    claimant: &Claimant<'_>,
    claimed: impl FnOnce(&str),
) -> Result<Allowed> {
    let info = opened.relay.as_ref().ok_or(Error::NoRelays)?;
    let identity = sheprd_tunnel::Identity::from_bytes(
        &URL_SAFE_NO_PAD
            .decode(&info.identity)
            .map_err(|_| Error::RelayInfo)?,
    )?;
    let route = RouteId::parse(&info.route).ok_or(Error::RelayInfo)?;
    let dial = sheprd_relay::route::pairing_dial_key(&opened.token);
    let (connection, (send, recv)) = pairing_dial(&info.relays, route, &dial).await?;
    let (initiator, first) = sheprd_tunnel::Initiator::pairing(
        &claimant.keys.tunnel,
        &identity.x25519(),
        &identity.fingerprint(),
        route.as_bytes(),
        &opened.token,
    )?;
    let tunnel = timeout(sheprd_tunnel::stream::connect(
        join(recv, send),
        initiator,
        &first,
        identity.x25519(),
    ))
    .await??;
    let tls = timeout(
        tokio_rustls::TlsConnector::from(Arc::new(tls_config(opened.fingerprint)?))
            .connect(ServerName::IpAddress(Ipv4Addr::LOCALHOST.into()), tunnel.io),
    )
    .await?
    .map_err(Error::Tls)?;
    let (mut sender, driver) = http1::handshake(TokioIo::new(tls)).await?;
    tokio::spawn(driver);
    let claim = request(
        &mut sender,
        Method::POST,
        "/api/pair/claim",
        Some(body(opened, claimant)?),
    )
    .await?;
    let claim: ClaimAnswer = serde_json::from_slice(&claim).map_err(|_| Error::ClaimFormat)?;
    // The daemon gives a nearby claim the code both screens agreed on; a
    // link's code is the daemon's, which the person compares on both.
    if let Some(code) = &opened.code
        && !auth::constant_time_eq(claim.code.as_bytes(), code.as_bytes())
    {
        return Err(Error::CodeMismatch);
    }
    if !valid_code(&claim.code) {
        return Err(Error::ClaimFormat);
    }
    claimed(&claim.code);
    let (login, fleet) = answer(&mut sender, &claim.claim).await?;
    drop(connection);
    let fleet = super::joining::checked(opened, claimant.fleet, fleet);
    Ok(Allowed {
        record: Record {
            computer: opened.computer.clone(),
            identity: info.identity.clone(),
            relays: info.relays.clone(),
            route: info.route.clone(),
            fleet: false,
            host: None,
            opening: false,
        },
        login,
        fleet,
    })
}

/// Six digits, as every code both screens show is.
fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit())
}

/// The claim's body: this computer's name, a device key the API wants,
/// its relay keys, and its SSH key.
fn body(opened: &Opened, claimant: &Claimant<'_>) -> Result<Vec<u8>> {
    // The API's own sign-in is never used by a desktop: a fresh key will do.
    let device = loop {
        if let Ok(device) = p256::SecretKey::from_slice(&auth::random_bytes::<32>()?) {
            break device;
        }
    };
    Ok(json!({
        "token": encode(opened.token.as_slice()),
        "name": claimant.name,
        "public_key": encode(&device.public_key().to_sec1_bytes()),
        "relay": {
            "dial": encode(&client::public(&claimant.keys.dial)),
            "tunnel": encode(&claimant.keys.tunnel.identity().x25519()),
        },
        "ssh": { "public_key": claimant.ssh_key },
        "fleet": claimant.fleet,
    })
    .to_string()
    .into_bytes())
}

#[derive(Deserialize)]
struct ClaimAnswer {
    claim: String,
    code: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
enum ClaimState {
    Approved {
        #[serde(default)]
        ssh: Option<Login>,
        #[serde(default)]
        fleet: Option<ApprovedFleet>,
    },
    Claimed,
    Denied,
    Expired,
}

/// Asks about the claim until its user answers.
async fn answer(
    sender: &mut http1::SendRequest<Full<Bytes>>,
    claim: &str,
) -> Result<(Login, Option<ApprovedFleet>)> {
    let deadline = Instant::now() + ANSWER;
    let path = format!("/api/pair/claim/{claim}");
    while Instant::now() < deadline {
        let body = request(sender, Method::GET, &path, None).await?;
        match serde_json::from_slice(&body).map_err(|_| Error::ClaimFormat)? {
            ClaimState::Approved {
                ssh: Some(login),
                fleet,
            } => return Ok((checked(login)?, fleet)),
            ClaimState::Approved { ssh: None, .. } => return Err(Error::NoLogin),
            ClaimState::Claimed => tokio::time::sleep(POLL).await,
            ClaimState::Denied => return Err(Error::Denied),
            ClaimState::Expired => return Err(Error::Expired),
        }
    }
    Err(Error::Expired)
}

/// `login` when its user and host keys can go into `ssh` options and a
/// known hosts file as they are.
fn checked(login: Login) -> Result<Login> {
    let key = |key: &String| {
        let mut fields = key.split(' ');
        matches!(
            (fields.next(), fields.next(), fields.next()),
            (Some(kind), Some(blob), None)
                if (kind.starts_with("ssh-") || kind.starts_with("ecdsa-"))
                    && kind.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'@'))
                    && base64::engine::general_purpose::STANDARD.decode(blob).is_ok()
        )
    };
    let valid = valid_user(&login.user)
        && !login.host_keys.is_empty()
        && login.host_keys.len() <= 8
        && login.host_keys.iter().all(key);
    valid.then_some(login).ok_or(Error::NoLogin)
}

/// One request on the claim connection; the body of a success, or the
/// error the API gave.
async fn request(
    sender: &mut http1::SendRequest<Full<Bytes>>,
    method: Method,
    path: &str,
    body: Option<Vec<u8>>,
) -> Result<Bytes> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "127.0.0.1");
    if body.is_some() {
        builder = builder.header(hyper::header::CONTENT_TYPE, "application/json");
    }
    let request = builder.body(Full::new(Bytes::from(body.unwrap_or_default())))?;
    let response = timeout(async {
        // Collecting the previous body can finish before the connection
        // driver advertises readiness for the next request. Wait; never replay
        // a claim that may already have spent its pairing token.
        sender.ready().await?;
        sender.send_request(request).await
    })
    .await??;
    let status = response.status();
    let body = timeout(Limited::new(response.into_body(), MAX_ANSWER).collect())
        .await?
        .map_err(|_| Error::ClaimFormat)?
        .to_bytes();
    if !status.is_success() {
        #[derive(Deserialize)]
        struct Refusal {
            error: String,
        }
        let reason = serde_json::from_slice::<Refusal>(&body)
            .map(|refusal| refusal.error.chars().take(200).collect())
            .unwrap_or_else(|_| status.to_string());
        return Err(Error::Claim(reason));
    }
    Ok(body)
}

/// The first relay that lets the pairing dial through, and the stream.
async fn pairing_dial(
    relays: &[String],
    route: RouteId,
    dial: &p256::ecdsa::SigningKey,
) -> Result<(quinn::Connection, (quinn::SendStream, quinn::RecvStream))> {
    let mut last = None;
    for relay in relays {
        let Ok(endpoint) = Endpoint::parse(relay) else {
            continue;
        };
        let connection = match timeout(client::reach(&endpoint)).await {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => {
                last = Some(error);
                continue;
            }
            Err(_) => continue,
        };
        // The relay learns the key moments after the token was sealed.
        let deadline = Instant::now() + DIAL_WAIT;
        loop {
            match client::dial(&connection, route, dial).await {
                Ok(halves) => return Ok((connection, halves)),
                Err(client::Error::Refused(Status::Offline)) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                Err(error) => {
                    last = Some(error);
                    break;
                }
            }
        }
    }
    Err(last.map_or(Error::NoRelays, Error::Relay))
}

async fn timeout<T>(step: impl Future<Output = T>) -> Result<T> {
    tokio::time::timeout(STEP, step)
        .await
        .map_err(|_| Error::Tunnel(sheprd_tunnel::Error::Closed))
}

#[cfg(test)]
mod tests;

/// TLS that trusts only the authority whose SHA-256 is `fingerprint`: the
/// other computer sends it after its certificate, and the code both
/// screens showed covered it.
fn tls_config(fingerprint: [u8; 32]) -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(PinnedAuthority {
        fingerprint,
        provider: Arc::clone(&provider),
    });
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| Error::Tls(std::io::Error::other(error)))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

#[derive(Debug)]
struct PinnedAuthority {
    fingerprint: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedAuthority {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let authority = intermediates
            .iter()
            .find(|certificate| Sha256::digest(certificate.as_ref()).as_slice() == self.fingerprint)
            .ok_or(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ))?;
        let mut roots = RootCertStore::empty();
        roots.add(authority.clone().into_owned())?;
        WebPkiServerVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&self.provider))
            .build()
            .map_err(|error| rustls::Error::General(error.to_string()))?
            .verify_server_cert(end_entity, &[], server_name, ocsp, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}
