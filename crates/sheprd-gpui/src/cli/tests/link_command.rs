use super::*;
use control::{ControlCommand, Invocation, LinkCommand, LinkTarget};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

#[test]
fn link_targets_and_pane_are_parsed() {
    for (verb, target) in [
        ("workspace", LinkTarget::Workspace),
        ("tab", LinkTarget::Tab),
        ("agent", LinkTarget::Agent),
    ] {
        assert_eq!(
            parse(&["link", verb, "--pane", "w1:p2"]).unwrap(),
            Invocation {
                command: ControlCommand::Link(LinkCommand {
                    target,
                    pane: Some("w1:p2".into()),
                }),
                json: false,
                window: None,
            }
        );
    }
}

#[test]
fn link_accepts_global_options_and_rejects_bad_targets() {
    let invocation = parse(&["link", "tab", "--json", "--window", "7"]).unwrap();
    assert!(invocation.json);
    assert_eq!(invocation.window, Some(7));
    assert_eq!(
        invocation.command,
        ControlCommand::Link(LinkCommand {
            target: LinkTarget::Tab,
            pane: None,
        })
    );
    assert!(matches!(
        parse(&["link", "other"]),
        Err(CliError::UnknownCommand { family: "link", .. })
    ));
    assert!(matches!(
        parse(&["link"]),
        Err(CliError::InvalidValue("workspace|tab|agent"))
    ));
}

#[test]
fn bare_link_is_a_command_family_not_a_url() {
    assert!(matches!(
        LaunchOptions::parse(["link", "tab"]).unwrap().mode,
        LaunchMode::Control(_)
    ));
}
