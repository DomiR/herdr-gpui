#![allow(clippy::unwrap_used)]
use crate::control::client::help::usage;

/// The usage line of `browser VERB`.
fn line(help: &str, verb: &str) -> String {
    let start = format!("sheprd browser {verb} ");
    help.lines()
        .find(|line| line.starts_with(&start))
        .unwrap()
        .to_owned()
}

#[test]
fn the_commands_that_can_take_focus_offer_it() {
    let help = usage(Some("browser"));
    for verb in ["open", "text", "screenshot"] {
        assert!(line(&help, verb).contains("[--focus]"), "{verb}");
    }
    for verb in ["navigate", "zoom"] {
        assert!(!line(&help, verb).contains("focus"), "{verb}");
    }
    // The old flag is still taken, as the default it now is.
    assert!(help.contains("--no-focus, the default, is accepted"));
}
