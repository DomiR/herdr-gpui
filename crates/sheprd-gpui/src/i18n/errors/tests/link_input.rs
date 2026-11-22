use super::*;
use crate::control::link_input::Error;

#[test]
fn every_link_input_variant_has_a_message() {
    assert_localized(&[
        Error::Empty,
        Error::Link(sheprd_remote_control::control::pairing_link::Error::Format),
        Error::Read(std::io::Error::other("gone")),
        Error::Terminal,
        Error::Text,
        Error::TooLong,
    ]);
}
