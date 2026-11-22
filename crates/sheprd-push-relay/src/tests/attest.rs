//! App Attest checks, against a test PKI shaped like Apple's: a P-384 root
//! and intermediate, and a P-256 leaf holding the nonce. Only Apple can sign
//! a real attestation; `real_attestation_from_a_phone` checks one by hand.

use super::*;
use crate::attest::{APPLE_ROOT, Attested, Environment, Error, KeyId};
use rcgen::{
    BasicConstraints, CertificateParams, CustomExtension, DnType, IsCa, Issuer as CertIssuer,
    KeyPair, PKCS_ECDSA_P256_SHA256, PKCS_ECDSA_P384_SHA384,
};
use sha2::{Digest as _, Sha256};

pub(super) const DEVELOPMENT: [u8; 16] = *b"appattestdevelop";
pub(super) const PRODUCTION: [u8; 16] = *b"appattest\0\0\0\0\0\0\0";
const NONCE_OID: &[u64] = &[1, 2, 840, 113_635, 100, 8, 2];
pub(super) const INTERMEDIATE: &str = "Test App Attestation CA 1";

/// A root and intermediate that sign attestations, like Apple's.
pub(super) struct Pki {
    pub(super) root: Vec<u8>,
    /// What the leaf names as its issuer; a test may rename it after the
    /// intermediate is signed, so the names no longer link.
    pub(super) intermediate: CertificateParams,
    intermediate_key: KeyPair,
    intermediate_der: Vec<u8>,
}

pub(super) fn ca(name: &str, is_ca: bool) -> CertificateParams {
    let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
    params.distinguished_name.push(DnType::CommonName, name);
    if is_ca {
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    }
    params
}

impl Pki {
    pub(super) fn new() -> Self {
        Self::with(
            IsCa::Ca(BasicConstraints::Unconstrained),
            ca(INTERMEDIATE, true),
        )
    }

    /// A PKI whose root is `root_ca` and whose intermediate is made from
    /// `intermediate`.
    pub(super) fn with(root_ca: IsCa, intermediate: CertificateParams) -> Self {
        let root_key = KeyPair::generate_for(&PKCS_ECDSA_P384_SHA384).unwrap();
        let mut root_params = ca("Test App Attestation Root CA", false);
        root_params.is_ca = root_ca;
        let root = root_params.self_signed(&root_key).unwrap();
        let intermediate_key = KeyPair::generate_for(&PKCS_ECDSA_P384_SHA384).unwrap();
        let intermediate_der = intermediate
            .signed_by(&intermediate_key, &CertIssuer::new(root_params, &root_key))
            .unwrap()
            .der()
            .to_vec();
        Self {
            root: root.der().to_vec(),
            intermediate,
            intermediate_key,
            intermediate_der,
        }
    }

    pub(super) fn verifier(&self) -> Verifier {
        Verifier::new(APP_ID, self.root.clone())
    }
}

/// One attestation's parts, each of which a test can spoil.
pub(super) struct Shape<'a> {
    pub(super) aaguid: [u8; 16],
    pub(super) app_id: &'a str,
    /// What the nonce covers: the challenge the phone attested over.
    pub(super) challenge: &'a [u8],
    pub(super) counter: u32,
    /// The credential id, when it should differ from the key id.
    pub(super) credential: Option<KeyId>,
    pub(super) fmt: &'a str,
    /// The leaf's basic constraints.
    pub(super) leaf_ca: IsCa,
    /// Extensions added to the leaf as they are, malformed or repeated.
    pub(super) leaf_extensions: Vec<CustomExtension>,
}

impl Default for Shape<'_> {
    fn default() -> Self {
        Self {
            aaguid: PRODUCTION,
            app_id: APP_ID,
            challenge: b"challenge",
            counter: 0,
            credential: None,
            fmt: "apple-appattest",
            leaf_ca: IsCa::NoCa,
            leaf_extensions: Vec::new(),
        }
    }
}

/// An attestation made by `pki`, and the key id it attests.
pub(super) fn attestation(pki: &Pki, shape: &Shape<'_>) -> (Vec<u8>, KeyId) {
    let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let key_id: KeyId = Sha256::digest(leaf_key.public_key_raw()).into();
    let mut auth_data = Sha256::digest(shape.app_id).to_vec();
    auth_data.push(0x40);
    auth_data.extend_from_slice(&shape.counter.to_be_bytes());
    auth_data.extend_from_slice(&shape.aaguid);
    auth_data.extend_from_slice(&32u16.to_be_bytes());
    auth_data.extend_from_slice(&shape.credential.unwrap_or(key_id));

    let mut nonce = Sha256::new();
    nonce.update(&auth_data);
    nonce.update(Sha256::digest(shape.challenge));
    let mut extension = vec![0x30, 0x24, 0xa1, 0x22, 0x04, 0x20];
    extension.extend_from_slice(&nonce.finalize());
    let mut leaf = ca("leaf", false);
    leaf.is_ca = shape.leaf_ca;
    leaf.custom_extensions = [CustomExtension::from_oid_content(NONCE_OID, extension)]
        .into_iter()
        .chain(shape.leaf_extensions.iter().cloned())
        .collect();
    let issuer = CertIssuer::new(pki.intermediate.clone(), &pki.intermediate_key);
    let leaf = leaf.signed_by(&leaf_key, &issuer).unwrap().der().to_vec();

    let object = map(&[
        (text("fmt"), text(shape.fmt)),
        (
            text("attStmt"),
            map(&[
                (
                    text("x5c"),
                    array(&[bytes(&leaf), bytes(&pki.intermediate_der)]),
                ),
                (text("receipt"), bytes(b"receipt")),
            ]),
        ),
        (text("authData"), bytes(&auth_data)),
    ]);
    (object, key_id)
}

/// The client data hash for `challenge`, as the relay computes it.
pub(super) fn hash(challenge: &[u8]) -> [u8; 32] {
    Sha256::digest(challenge).into()
}

fn head(major: u8, length: usize) -> Vec<u8> {
    match length {
        0..=23 => vec![major << 5 | u8::try_from(length).unwrap()],
        24..=255 => vec![major << 5 | 24, u8::try_from(length).unwrap()],
        _ => {
            let mut head = vec![major << 5 | 25];
            head.extend_from_slice(&u16::try_from(length).unwrap().to_be_bytes());
            head
        }
    }
}

fn bytes(value: &[u8]) -> Vec<u8> {
    [head(2, value.len()), value.to_vec()].concat()
}

fn text(value: &str) -> Vec<u8> {
    [head(3, value.len()), value.as_bytes().to_vec()].concat()
}

fn array(items: &[Vec<u8>]) -> Vec<u8> {
    [head(4, items.len()), items.concat()].concat()
}

fn map(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let mut out = head(5, entries.len());
    for (key, value) in entries {
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    out
}

pub(super) fn verify(pki: &Pki, shape: &Shape<'_>) -> Result<Attested, Error> {
    let (object, key_id) = attestation(pki, shape);
    pki.verifier()
        .verify(&object, &key_id, &hash(b"challenge"), NOW_S)
}

#[test]
fn a_good_attestation_yields_its_key_and_environment() {
    let pki = Pki::new();
    let (object, key_id) = attestation(&pki, &Shape::default());
    let attested = pki
        .verifier()
        .verify(&object, &key_id, &hash(b"challenge"), NOW_S)
        .unwrap();
    assert_eq!(attested.environment, Environment::Production);
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(attested.public_key)),
        key_id
    );

    let development = Shape {
        aaguid: DEVELOPMENT,
        ..Shape::default()
    };
    assert_eq!(
        verify(&pki, &development).unwrap().environment,
        Environment::Development
    );
}

#[test]
fn each_spoiled_part_is_refused_for_its_own_reason() {
    let pki = Pki::new();
    let cases = [
        (
            Shape {
                challenge: b"another",
                ..Shape::default()
            },
            Error::Nonce,
        ),
        (
            Shape {
                app_id: "ABCDE12345.other.app",
                ..Shape::default()
            },
            Error::App,
        ),
        (
            Shape {
                counter: 1,
                ..Shape::default()
            },
            Error::Counter,
        ),
        (
            Shape {
                aaguid: [0; 16],
                ..Shape::default()
            },
            Error::Environment,
        ),
        (
            Shape {
                credential: Some([7; 32]),
                ..Shape::default()
            },
            Error::Credential,
        ),
        (
            Shape {
                fmt: "packed",
                ..Shape::default()
            },
            Error::Format,
        ),
    ];
    for (shape, expected) in cases {
        assert_eq!(verify(&pki, &shape), Err(expected));
    }
}

#[test]
fn an_attestation_for_another_key_is_refused() {
    let pki = Pki::new();
    let (object, _) = attestation(&pki, &Shape::default());
    let verified = pki
        .verifier()
        .verify(&object, &[9; 32], &hash(b"challenge"), NOW_S);
    assert_eq!(verified, Err(Error::Key));
}

#[test]
fn only_a_chain_to_the_root_at_a_valid_time_is_taken() {
    let pki = Pki::new();
    let (object, key_id) = attestation(&pki, &Shape::default());
    let stranger = Pki::new();
    let check =
        |verifier: Verifier, now_s| verifier.verify(&object, &key_id, &hash(b"challenge"), now_s);
    assert_eq!(check(stranger.verifier(), NOW_S), Err(Error::Chain));
    // rcgen's certificates are valid from 1975 to 4096.
    assert_eq!(check(pki.verifier(), 0), Err(Error::Chain));
    assert_eq!(
        check(Verifier::new(APP_ID, b"not a certificate".to_vec()), NOW_S),
        Err(Error::Certificate)
    );

    let not_ca = Pki::with(
        IsCa::Ca(BasicConstraints::Unconstrained),
        ca(INTERMEDIATE, false),
    );
    assert_eq!(verify(&not_ca, &Shape::default()), Err(Error::Chain));
}

#[test]
fn malformed_cbor_is_refused_before_any_certificate_is_read() {
    let pki = Pki::new();
    let (good, key_id) = attestation(&pki, &Shape::default());
    let deep = [vec![0x81; 8], vec![0x00]].concat();
    let long = [vec![0x91], vec![0x00; 17]].concat();
    let cases: [&[u8]; 8] = [
        b"",
        &[0x9f, 0xff],                   // indefinite length
        &[0xc0, 0x00],                   // a tag
        &[0xfb, 0, 0, 0, 0, 0, 0, 0, 0], // a float
        &[0x5a, 0xff, 0xff, 0xff, 0xff], // bytes longer than the input
        &deep,
        &long,
        &[good.as_slice(), &[0x00]].concat(),
    ];
    for case in cases {
        let verified = pki
            .verifier()
            .verify(case, &key_id, &hash(b"challenge"), NOW_S);
        assert_eq!(verified, Err(Error::Cbor), "{case:02x?}");
    }
    let truncated = &good[..good.len() - 1];
    assert_eq!(
        pki.verifier()
            .verify(truncated, &key_id, &hash(b"challenge"), NOW_S),
        Err(Error::Cbor)
    );
}

#[test]
fn apples_root_is_embedded() {
    let (rest, root) = x509_parser::parse_x509_certificate(APPLE_ROOT).unwrap();
    assert!(rest.is_empty());
    assert!(
        root.subject()
            .to_string()
            .contains("Apple App Attestation Root CA")
    );
    assert!(root.is_ca());
}

/// An attestation from an iPhone 15 Pro, made by the spike app's Attest tab
/// (`ios/Spikes`, a development build of `VJAHQVZ96V.so.pen.herdr-spikes`)
/// on 2026-11-21. Its fields are standard base64, as the app saved them.
const IPHONE: &[u8] = include_bytes!("fixtures/iphone-attestation.json");
const IPHONE_APP: &str = "VJAHQVZ96V.so.pen.herdr-spikes";
/// When it was made: Apple's leaf is good from a day before to two after.
const IPHONE_AT_S: u64 = 1_795_247_105;

fn iphone() -> (Vec<u8>, KeyId, Vec<u8>) {
    use base64::engine::general_purpose::STANDARD;
    let record: serde_json::Value = serde_json::from_slice(IPHONE).unwrap();
    let field = |name: &str| STANDARD.decode(record[name].as_str().unwrap()).unwrap();
    let key_id = field("key_id").try_into().unwrap();
    (field("attestation"), key_id, field("challenge"))
}

#[test]
fn a_real_attestation_from_an_iphone_checks_against_apples_root() {
    let (object, key_id, challenge) = iphone();
    let apple = Verifier::apple(IPHONE_APP);
    let attested = apple
        .verify(&object, &key_id, &hash(&challenge), IPHONE_AT_S)
        .unwrap();
    assert_eq!(attested.environment, Environment::Development);
    assert_eq!(
        <[u8; 32]>::from(Sha256::digest(attested.public_key)),
        key_id
    );

    let three_days_later = IPHONE_AT_S + 3 * 24 * 60 * 60;
    let cases = [
        (
            apple.verify(&object, &key_id, &hash(b"another"), IPHONE_AT_S),
            Error::Nonce,
        ),
        (
            apple.verify(&object, &key_id, &hash(&challenge), three_days_later),
            Error::Chain,
        ),
        (
            Verifier::apple(APP_ID).verify(&object, &key_id, &hash(&challenge), IPHONE_AT_S),
            Error::App,
        ),
        (
            Pki::new()
                .verifier()
                .verify(&object, &key_id, &hash(&challenge), IPHONE_AT_S),
            Error::Chain,
        ),
    ];
    for (verified, expected) in cases {
        assert_eq!(verified, Err(expected));
    }
}
