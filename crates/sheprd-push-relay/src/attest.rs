//! App Attest: the proof that a key belongs to a genuine copy of the iOS
//! app on a real Apple device, checked as Apple describes in "Validating
//! apps that connect to your server". A phone sends one attestation per key
//! and gets a [`crate::credential`] for it.
//!
//! The steps, in order: the certificates chain to Apple's App Attest root
//! (names linked, each issuer a CA allowed to sign at its depth, the root
//! self-signed, the leaf not a CA, all valid now); the leaf's nonce
//! extension covers the authenticator data and the hash of the relay's
//! challenge; the leaf's key hashes to the key id; the authenticator data
//! names this app, counts zero, says which environment attested, and
//! carries the same key id. The receipt Apple includes for
//! fraud assessment is not used.

mod cbor;
mod error;

pub use error::Error;

use sha2::{Digest as _, Sha256};
use x509_parser::{
    certificate::X509Certificate,
    extensions::ParsedExtension,
    oid_registry::{OID_X509_EXT_BASIC_CONSTRAINTS, Oid},
    prelude::FromDer as _,
    time::ASN1Time,
};

/// Apple App Attestation Root CA in DER, from
/// <https://www.apple.com/certificateauthority/Apple_App_Attestation_Root_CA.pem>.
/// SHA-256 fingerprint `1C:B9:82:3B:A2:8B:A6:AD:2D:33:A0:06:94:1D:E2:AE:4F:51:3E:F1:D4:E8:31:B9:F7:E0:FA:7B:62:42:C9:32`.
pub(crate) const APPLE_ROOT: &[u8] = include_bytes!("attest/apple-app-attestation-root.cer");

/// The leaf certificate's extension holding the nonce.
const NONCE_OID: &[u64] = &[1, 2, 840, 113_635, 100, 8, 2];
/// The DER around the nonce in that extension: a sequence holding `[1]`
/// holding an octet string of 32 bytes.
const NONCE_PREFIX: [u8; 6] = [0x30, 0x24, 0xa1, 0x22, 0x04, 0x20];

/// Authenticator data: `rpIdHash (32) || flags (1) || counter (4) ||
/// aaguid (16) || credentialIdLength (2) || credentialId || ...`.
const RP_ID_HASH: std::ops::Range<usize> = 0..32;
const COUNTER: std::ops::Range<usize> = 33..37;
const AAGUID: std::ops::Range<usize> = 37..53;
const CREDENTIAL_LENGTH: std::ops::Range<usize> = 53..55;
const CREDENTIAL: usize = 55;

/// A key id: the SHA-256 of the attested public key, as the app's
/// `DCAppAttestService` names it.
pub type KeyId = [u8; 32];

/// Which App Attest environment vouched for a key, from the authenticator
/// data's AAGUID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Environment {
    /// Builds run from Xcode.
    Development,
    /// TestFlight and App Store builds.
    Production,
}

impl Environment {
    fn from_aaguid(aaguid: &[u8]) -> Option<Self> {
        match aaguid {
            b"appattestdevelop" => Some(Self::Development),
            b"appattest\0\0\0\0\0\0\0" => Some(Self::Production),
            _ => None,
        }
    }
}

/// What a valid attestation proves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Attested {
    pub environment: Environment,
    /// The attested key, as an uncompressed SEC1 point, for checking the
    /// app's later assertions.
    pub public_key: [u8; 65],
}

/// Checks attestations for one app against one root.
pub struct Verifier {
    app_id_hash: [u8; 32],
    root: Vec<u8>,
}

impl Verifier {
    /// For `app_id` (`TEAM_ID.bundle.id`), against Apple's root.
    #[must_use]
    pub fn apple(app_id: &str) -> Self {
        Self::new(app_id, APPLE_ROOT.to_vec())
    }

    /// For `app_id`, against the root certificate `root_der`. Tests use
    /// their own root, since only Apple can sign a real attestation.
    #[must_use]
    pub fn new(app_id: &str, root_der: Vec<u8>) -> Self {
        Self {
            app_id_hash: Sha256::digest(app_id).into(),
            root: root_der,
        }
    }

    /// Checks `attestation` for `key_id`, made over `client_data_hash` (the
    /// SHA-256 of the relay's challenge), at `now_s`.
    pub fn verify(
        &self,
        attestation: &[u8],
        key_id: &KeyId,
        client_data_hash: &[u8; 32],
        now_s: u64,
    ) -> Result<Attested, Error> {
        let object = cbor::parse(attestation)?;
        if object.get("fmt") != Some(&cbor::Value::Text("apple-appattest")) {
            return Err(Error::Format);
        }
        let auth_data = object.get("authData").and_then(cbor::Value::bytes);
        let chain = object
            .get("attStmt")
            .and_then(|statement| statement.get("x5c"));
        let (Some(auth_data), Some(cbor::Value::Array(chain))) = (auth_data, chain) else {
            return Err(Error::Format);
        };
        let [Some(leaf), Some(intermediate)] =
            [chain.first(), chain.get(1)].map(|cert| cert.and_then(cbor::Value::bytes))
        else {
            return Err(Error::Format);
        };

        let leaf = certificate(leaf)?;
        let intermediate = certificate(intermediate)?;
        let root = certificate(&self.root)?;
        let now = i64::try_from(now_s)
            .ok()
            .and_then(|now| ASN1Time::from_timestamp(now).ok())
            .ok_or(Error::Chain)?;
        // The root signs itself with one CA (the intermediate) below it.
        let chained = issued_by(&root, &root, 1, now)
            && issued_by(&intermediate, &root, 0, now)
            && issued_by(&leaf, &intermediate, 0, now)
            && !is_ca(&leaf);
        if !chained {
            return Err(Error::Chain);
        }

        let mut nonce = Sha256::new();
        nonce.update(auth_data);
        nonce.update(client_data_hash);
        let mut expected = NONCE_PREFIX.to_vec();
        expected.extend_from_slice(&nonce.finalize());
        let oid = Oid::from(NONCE_OID).map_err(|_| Error::Nonce)?;
        let extension = leaf
            .get_extension_unique(&oid)
            .ok()
            .flatten()
            .ok_or(Error::Nonce)?;
        if extension.value != expected.as_slice() {
            return Err(Error::Nonce);
        }

        let public_key: [u8; 65] = leaf
            .public_key()
            .subject_public_key
            .data
            .as_ref()
            .try_into()
            .map_err(|_| Error::Key)?;
        if Sha256::digest(public_key).as_slice() != key_id {
            return Err(Error::Key);
        }

        if auth_data.get(RP_ID_HASH) != Some(self.app_id_hash.as_slice()) {
            return Err(Error::App);
        }
        if auth_data.get(COUNTER) != Some([0; 4].as_slice()) {
            return Err(Error::Counter);
        }
        let environment = auth_data
            .get(AAGUID)
            .and_then(Environment::from_aaguid)
            .ok_or(Error::Environment)?;
        let length = auth_data
            .get(CREDENTIAL_LENGTH)
            .and_then(|bytes| bytes.try_into().ok())
            .map(|bytes| usize::from(u16::from_be_bytes(bytes)));
        let credential = length.and_then(|length| auth_data.get(CREDENTIAL..CREDENTIAL + length));
        if credential != Some(key_id.as_slice()) {
            return Err(Error::Credential);
        }
        Ok(Attested {
            environment,
            public_key,
        })
    }
}

/// Whether `child` is `parent`'s: the names link, `parent` may sign with
/// `below` CAs under it, its key made the signature, and both certificates
/// are valid at `now`. The same checks as `ios/Spikes/attest-check.py`.
fn issued_by(
    child: &X509Certificate<'_>,
    parent: &X509Certificate<'_>,
    below: u32,
    now: ASN1Time,
) -> bool {
    child.issuer() == parent.subject()
        && may_issue(parent, below)
        && child.verify_signature(Some(parent.public_key())).is_ok()
        && child.validity().is_valid_at(now)
        && parent.validity().is_valid_at(now)
}

/// Whether `parent` may sign certificates with `below` CAs under it: it is
/// a CA, its path length (if any) allows them, and its key usage (if any)
/// includes signing certificates. A repeated or unreadable extension
/// refuses: `basic_constraints` reads an unreadable one as absent, and
/// `key_usage` fails on it.
fn may_issue(parent: &X509Certificate<'_>, below: u32) -> bool {
    let Ok(Some(constraints)) = parent.basic_constraints() else {
        return false;
    };
    let constraints = constraints.value;
    if !constraints.ca
        || constraints
            .path_len_constraint
            .is_some_and(|length| below > length)
    {
        return false;
    }
    match parent.key_usage() {
        Ok(None) => true,
        Ok(Some(usage)) => usage.value.key_cert_sign(),
        Err(_) => false,
    }
}

/// Whether `certificate` may be a CA: anything but no basic constraints or
/// one readable extension saying it is not. x509-parser's
/// `basic_constraints` reads an unparsable extension as absent, so the
/// extension is looked up by its OID instead.
fn is_ca(certificate: &X509Certificate<'_>) -> bool {
    match certificate.get_extension_unique(&OID_X509_EXT_BASIC_CONSTRAINTS) {
        Ok(None) => false,
        Ok(Some(extension)) => !matches!(
            extension.parsed_extension(),
            ParsedExtension::BasicConstraints(constraints) if !constraints.ca
        ),
        Err(_) => true,
    }
}

/// One DER certificate with nothing after it.
fn certificate(der: &[u8]) -> Result<X509Certificate<'_>, Error> {
    match X509Certificate::from_der(der) {
        Ok(([], certificate)) => Ok(certificate),
        _ => Err(Error::Certificate),
    }
}
