use super::*;
use control::{ControlCommand, FleetCommand, Invocation};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> ControlCommand {
    parse(args).unwrap().command
}

#[test]
fn fleet_reports_status_by_default() {
    for args in [&["fleet"][..], &["fleet", "status"]] {
        assert_eq!(command(args), ControlCommand::Fleet(FleetCommand::Status));
    }
    let json = parse(&["fleet", "hosts", "--json", "--window", "3"]).unwrap();
    assert!(json.json);
    assert_eq!(json.window, Some(3));
    assert_eq!(json.command, ControlCommand::Fleet(FleetCommand::Hosts));
    assert_eq!(
        command(&["fleet", "members"]),
        ControlCommand::Fleet(FleetCommand::Members)
    );
    assert_eq!(
        command(&["fleet", "open"]),
        ControlCommand::Fleet(FleetCommand::Open)
    );
}

#[test]
fn fleet_removals_name_a_member_or_host() {
    for verb in ["remove", "remove-member"] {
        assert_eq!(
            command(&["fleet", verb, "old", "laptop"]),
            ControlCommand::Fleet(FleetCommand::Remove {
                member: "old laptop".into()
            })
        );
    }
    assert_eq!(
        command(&["fleet", "remove-host", "build-01"]),
        ControlCommand::Fleet(FleetCommand::RemoveHost {
            host: "build-01".into()
        })
    );
    assert!(matches!(
        parse(&["fleet", "remove"]),
        Err(CliError::MissingArgument { .. })
    ));
    assert_eq!(
        command(&["fleet", "scrub", "me@build-01"]),
        ControlCommand::Fleet(FleetCommand::Scrub {
            machine: "me@build-01".into()
        })
    );
    assert!(parse(&["fleet", "found"]).is_err());
}
