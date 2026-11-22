//! Each link of the certificate chain is checked as `ios/Spikes/attest-check.py`
//! checks it: the names link, each issuer may sign at its depth, and the
//! leaf is not a CA. A repeated or unreadable constraint refuses.

use super::attest::{INTERMEDIATE, Pki, Shape, ca, verify};
use crate::attest::Error;
use rcgen::{BasicConstraints, CustomExtension, DnType, IsCa, KeyUsagePurpose};

const BASIC_CONSTRAINTS: &[u64] = &[2, 5, 29, 19];
const KEY_USAGE: &[u64] = &[2, 5, 29, 15];
/// `BasicConstraints ::= SEQUENCE { cA TRUE }`.
const CA: &[u8] = &[0x30, 0x03, 0x01, 0x01, 0xff];
/// `BasicConstraints ::= SEQUENCE {}`: not a CA.
const NOT_CA: &[u8] = &[0x30, 0x00];
/// `KeyUsage ::= BIT STRING { keyCertSign }`.
const CERT_SIGN: &[u8] = &[0x03, 0x02, 0x02, 0x04];
/// An ASN.1 NULL, which is neither extension.
const UNREADABLE: &[u8] = &[0x05, 0x00];

fn unconstrained() -> IsCa {
    IsCa::Ca(BasicConstraints::Unconstrained)
}

fn extension(oid: &[u64], content: &[u8]) -> CustomExtension {
    CustomExtension::from_oid_content(oid, content.to_vec())
}

/// An intermediate made by `edit` from an ordinary CA.
fn intermediate(edit: impl FnOnce(&mut rcgen::CertificateParams)) -> Pki {
    let mut params = ca(INTERMEDIATE, true);
    edit(&mut params);
    Pki::with(unconstrained(), params)
}

#[test]
fn an_intermediate_with_key_usage_that_allows_signing_certificates_is_taken() {
    let mut intermediate = ca(INTERMEDIATE, true);
    intermediate.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let pki = Pki::with(IsCa::Ca(BasicConstraints::Constrained(1)), intermediate);
    assert!(verify(&pki, &Shape::default()).is_ok());
}

#[test]
fn an_intermediate_whose_key_usage_lacks_certificate_signing_is_refused() {
    let mut intermediate = ca(INTERMEDIATE, true);
    intermediate.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    let pki = Pki::with(unconstrained(), intermediate);
    assert_eq!(verify(&pki, &Shape::default()), Err(Error::Chain));
}

#[test]
fn a_root_whose_path_length_forbids_the_intermediate_is_refused() {
    let pki = Pki::with(
        IsCa::Ca(BasicConstraints::Constrained(0)),
        ca(INTERMEDIATE, true),
    );
    assert_eq!(verify(&pki, &Shape::default()), Err(Error::Chain));
}

#[test]
fn a_leaf_that_is_a_ca_is_refused() {
    let shape = Shape {
        leaf_ca: unconstrained(),
        ..Shape::default()
    };
    assert_eq!(verify(&Pki::new(), &shape), Err(Error::Chain));
}

#[test]
fn a_leaf_whose_issuer_is_not_the_intermediate_is_refused() {
    // The leaf is still signed by the intermediate's key; only its issuer
    // name differs from the intermediate's subject.
    let mut pki = Pki::new();
    pki.intermediate.distinguished_name = rcgen::DistinguishedName::new();
    pki.intermediate
        .distinguished_name
        .push(DnType::CommonName, "Another CA");
    assert_eq!(verify(&pki, &Shape::default()), Err(Error::Chain));
}

#[test]
fn a_leaf_that_says_it_is_not_a_ca_is_taken() {
    let shape = Shape {
        leaf_ca: IsCa::ExplicitNoCa,
        ..Shape::default()
    };
    assert!(verify(&Pki::new(), &shape).is_ok());
}

#[test]
fn a_leaf_with_unreadable_or_repeated_basic_constraints_is_refused() {
    for extensions in [
        vec![extension(BASIC_CONSTRAINTS, UNREADABLE)],
        vec![
            extension(BASIC_CONSTRAINTS, NOT_CA),
            extension(BASIC_CONSTRAINTS, NOT_CA),
        ],
    ] {
        let shape = Shape {
            leaf_extensions: extensions,
            ..Shape::default()
        };
        assert_eq!(verify(&Pki::new(), &shape), Err(Error::Chain));
    }
}

#[test]
fn an_intermediate_with_unreadable_or_repeated_extensions_is_refused() {
    let cases = [
        // Unreadable basic constraints in place of rcgen's own.
        intermediate(|params| {
            params.is_ca = IsCa::NoCa;
            params.custom_extensions = vec![extension(BASIC_CONSTRAINTS, UNREADABLE)];
        }),
        // A second basic constraints beside rcgen's own.
        intermediate(|params| {
            params.custom_extensions = vec![extension(BASIC_CONSTRAINTS, CA)];
        }),
        intermediate(|params| {
            params.custom_extensions = vec![extension(KEY_USAGE, UNREADABLE)];
        }),
        intermediate(|params| {
            params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
            params.custom_extensions = vec![extension(KEY_USAGE, CERT_SIGN)];
        }),
    ];
    for pki in cases {
        assert_eq!(verify(&pki, &Shape::default()), Err(Error::Chain));
    }
}

#[test]
fn an_intermediate_with_hand_written_extensions_is_taken() {
    // The same DER as the refused cases above, once each, is accepted, so
    // those refusals come from the repetition or the unreadable value.
    let pki = intermediate(|params| {
        params.is_ca = IsCa::NoCa;
        params.custom_extensions = vec![
            extension(BASIC_CONSTRAINTS, CA),
            extension(KEY_USAGE, CERT_SIGN),
        ];
    });
    assert!(verify(&pki, &Shape::default()).is_ok());
}
