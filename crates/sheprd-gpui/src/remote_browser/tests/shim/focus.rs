use super::*;

#[test]
fn a_page_from_a_host_opens_in_the_background_unless_asked() {
    for (args, focus) in [
        (&["browser", "open", "localhost:3000"][..], false),
        (&["browser", "open", "--no-focus", "localhost:3000"], false),
        (&["browser", "open", "--focus", "localhost:3000"], true),
        (&["browser", "open", "localhost:3000", "--focus"], true),
    ] {
        let host = Host::new();
        let output = host.run(args, &[]);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert_eq!(
            request(&host).action,
            Action::Open {
                address: "localhost:3000".into(),
                focus
            },
            "{args:?}"
        );
    }
}

#[test]
fn the_host_help_offers_focus() {
    let output = Host::new().run(&["browser", "--help"], &[]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("[--focus]"), "{text}");
}
