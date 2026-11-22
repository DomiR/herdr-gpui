use super::*;

#[test]
fn create_accepts_shared_options_but_no_confirmation_bypass() {
    assert_eq!(
        LaunchOptions::parse(["pr", "create", "--json", "--window", "7"])
            .unwrap()
            .mode,
        LaunchMode::Control(control::Invocation {
            command: control::ControlCommand::Pr(control::PrCommand::Create),
            json: true,
            window: Some(7),
        })
    );
    for extra in ["extra", "--confirmed", "--yes", "--token"] {
        assert!(
            LaunchOptions::parse(["pr", "create", extra]).is_err(),
            "{extra}"
        );
    }
}
