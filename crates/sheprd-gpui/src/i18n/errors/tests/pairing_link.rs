use super::*;
use sheprd_remote_control::control::pairing_link::Error;

#[test]
fn every_pairing_link_variant_has_a_message() {
    assert_localized(&[
        Error::Format,
        Error::NoRelays,
        Error::TooLong,
        Error::Version,
    ]);
}
