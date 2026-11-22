use super::*;

const FLEET: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

#[test]
fn fleet_apply_and_bundle_parse() {
    assert_eq!(
        parse(args(&["fleet", "apply", "--state-dir", "/tmp/s"])).unwrap(),
        Invocation::Fleet {
            state_dir: Some("/tmp/s".into()),
            action: FleetAction::Apply,
        }
    );
    assert_eq!(
        parse(args(&["fleet", "bundle", "--fleet", FLEET])).unwrap(),
        Invocation::Fleet {
            state_dir: None,
            action: FleetAction::Bundle {
                fleet: FLEET.to_owned()
            },
        }
    );
}

#[test]
fn fleet_intro_and_scrub_parse() {
    assert_eq!(
        parse(args(&["fleet", "intro"])).unwrap(),
        Invocation::Fleet {
            state_dir: None,
            action: FleetAction::Intro,
        }
    );
    assert_eq!(
        parse(args(&["fleet", "scrub"])).unwrap(),
        Invocation::Fleet {
            state_dir: None,
            action: FleetAction::Scrub,
        }
    );
}

#[test]
fn fleet_refuses_what_it_does_not_take() {
    assert_eq!(
        parse(args(&["fleet", "bundle"])).unwrap_err(),
        ParseError::Missing("--fleet")
    );
    assert_eq!(
        parse(args(&["fleet", "bundle", "--fleet", "not-hex"])).unwrap_err(),
        ParseError::Value { option: "--fleet" }
    );
    assert!(matches!(
        parse(args(&["fleet", "apply", "--fleet", FLEET])),
        Err(ParseError::Unknown(_))
    ));
    assert!(matches!(
        parse(args(&["fleet", "join"])),
        Err(ParseError::Unknown(_))
    ));
    assert_eq!(
        parse(args(&["fleet"])).unwrap_err(),
        ParseError::Missing("apply, bundle, intro, or scrub")
    );
}
