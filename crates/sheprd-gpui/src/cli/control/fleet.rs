//! `fleet`: your computers and phones as one fleet.
use super::{Args, CliError, unknown};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetCommand {
    /// Members and hosts, as the daemon keeps them.
    Status,
    Members,
    Hosts,
    /// Show Settings > Fleet.
    Open,
    /// Ask in Settings > Fleet to remove a member, by ID or name.
    Remove {
        member: String,
    },
    /// Ask in Settings > Fleet to remove a host everywhere, by ID or name.
    RemoveHost {
        host: String,
    },
    /// Ask in Settings > Fleet to remove every way a fleet reaches a
    /// machine, by device ID or name.
    Scrub {
        machine: String,
    },
}

pub(super) fn fleet(args: &mut Args) -> Result<FleetCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(FleetCommand::Status);
    };
    Ok(match verb.to_str() {
        Some("status") => FleetCommand::Status,
        Some("members") => FleetCommand::Members,
        Some("hosts") => FleetCommand::Hosts,
        Some("open") => FleetCommand::Open,
        // A name may be typed unquoted, as `fleet remove old laptop`.
        Some("remove" | "remove-member") => FleetCommand::Remove {
            member: args.words()?.ok_or(CliError::MissingArgument {
                command: "fleet remove",
                what: "MEMBER",
            })?,
        },
        Some("remove-host") => FleetCommand::RemoveHost {
            host: args.words()?.ok_or(CliError::MissingArgument {
                command: "fleet remove-host",
                what: "HOST",
            })?,
        },
        Some("scrub") => FleetCommand::Scrub {
            machine: args.words()?.ok_or(CliError::MissingArgument {
                command: "fleet scrub",
                what: "MACHINE",
            })?,
        },
        _ => return Err(unknown("fleet", verb)),
    })
}
