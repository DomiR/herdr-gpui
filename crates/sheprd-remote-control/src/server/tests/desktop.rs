use super::*;

#[test]
fn desktop_pair_needs_a_name() {
    assert_eq!(
        parse(args(&[
            "desktop-pair",
            "--name",
            "Laptop",
            "--state-dir",
            "/s"
        ]))
        .unwrap(),
        Invocation::DesktopPair {
            state_dir: Some(PathBuf::from("/s")),
            name: "Laptop".to_owned(),
            link: false,
        }
    );
    assert!(matches!(
        parse(args(&["desktop-pair"])),
        Err(ParseError::Missing("--name"))
    ));
}

#[test]
fn desktop_pair_takes_a_link_from_stdin_when_asked() {
    assert_eq!(
        parse(args(&["desktop-pair", "--name", "Laptop", "--link"])).unwrap(),
        Invocation::DesktopPair {
            state_dir: None,
            name: "Laptop".to_owned(),
            link: true,
        }
    );
    assert!(matches!(
        parse(args(&[
            "relay-connect",
            "--link",
            "r-0123456789abcdef.sheprd.invalid"
        ])),
        Err(ParseError::Unknown(_))
    ));
}

#[test]
fn relay_connect_takes_one_host() {
    assert_eq!(
        parse(args(&[
            "relay-connect",
            "--state-dir",
            "/s",
            "r-0123456789abcdef.sheprd.invalid"
        ]))
        .unwrap(),
        Invocation::RelayConnect {
            state_dir: Some(PathBuf::from("/s")),
            host: "r-0123456789abcdef.sheprd.invalid".to_owned(),
        }
    );
    assert!(matches!(
        parse(args(&["relay-connect"])),
        Err(ParseError::Missing("HOST"))
    ));
    assert!(matches!(
        parse(args(&["relay-connect", "a", "b"])),
        Err(ParseError::Unknown(_))
    ));
    assert!(matches!(
        parse(args(&["relay-connect", "--name", "x", "a"])),
        Err(ParseError::Unknown(_))
    ));
}
