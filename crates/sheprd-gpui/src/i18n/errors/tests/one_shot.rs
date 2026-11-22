use super::*;
use sheprd_client::one_shot::Error;

#[test]
fn every_one_shot_variant_has_a_message() {
    assert_localized(&[
        Error::Cancelled,
        Error::Empty,
        Error::Failed,
        Error::NoAgent,
        Error::Start(std::io::Error::other("x")),
        Error::TimedOut,
    ]);
}
