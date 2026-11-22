/// Why an attestation was refused. The relay tells a caller only that it
/// was; these are for tests and the operator's own checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the attestation's app is not this relay's app")]
    App,
    #[error("the attestation is not valid CBOR of the kind App Attest sends")]
    Cbor,
    #[error("a certificate could not be parsed")]
    Certificate,
    #[error("the certificates do not chain to the App Attest root at this time")]
    Chain,
    #[error("the attestation's counter is not zero")]
    Counter,
    #[error("the attestation is for another key")]
    Credential,
    #[error("the attestation names no App Attest environment this relay knows")]
    Environment,
    #[error("the attestation is not an App Attest object")]
    Format,
    #[error("the attested key does not match its key id")]
    Key,
    #[error("the attestation does not cover this challenge")]
    Nonce,
    // keep-sorted end
}
