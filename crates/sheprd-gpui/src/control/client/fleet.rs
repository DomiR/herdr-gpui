//! `fleet`: the fleet as this computer's daemon keeps it, and Settings >
//! Fleet's questions.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::FleetCommand,
    control::protocol::{
        Request, Response,
        fleet::{FleetOp, Reply},
    },
};
use sheprd_remote_control::control::fleet::{HostKind, Kind, View};

pub(super) fn fleet(command: FleetCommand, context: &Context) -> crate::Result<Output> {
    let (op, part) = match command {
        FleetCommand::Status => (FleetOp::Status, Part::All),
        FleetCommand::Members => (FleetOp::Status, Part::Members),
        FleetCommand::Hosts => (FleetOp::Status, Part::Hosts),
        FleetCommand::Open => (FleetOp::Open, Part::All),
        FleetCommand::Remove { member } => (FleetOp::Remove { member }, Part::All),
        FleetCommand::RemoveHost { host } => (FleetOp::RemoveHost { host }, Part::All),
        FleetCommand::Scrub { machine } => (FleetOp::Scrub { machine }, Part::All),
    };
    match context.call(Request::Fleet, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::Fleet { reply } => output(&reply, part),
        response => Err(unexpected(response)),
    }
}

/// Which part of the fleet a command prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Part {
    All,
    Members,
    Hosts,
}

/// What a reply prints, as text and as JSON.
pub(super) fn output(reply: &Reply, part: Part) -> crate::Result<Output> {
    let Reply::Status { view, error } = reply;
    let Some(view) = view else {
        let json = serde_json::json!({ "fleet": null, "error": error });
        // A daemon that could not be read is a failure, not an answer that
        // this computer is in no fleet.
        return Ok(match error {
            Some(error) => {
                Output::data(format!("Could not read the fleet: {error}"), &json)?.failed()
            }
            None => Output::data("This computer is in no fleet", &json)?.nothing(),
        });
    };
    let json = match part {
        Part::All => serde_json::to_value(view.as_ref())?,
        Part::Members => serde_json::json!({ "members": view.members }),
        Part::Hosts => serde_json::json!({ "hosts": view.hosts }),
    };
    Output::data(text(view, part), &json)
}

fn text(view: &View, part: Part) -> String {
    let name = |id: &str| {
        view.members
            .iter()
            .find(|member| member.id == id)
            .map_or_else(
                || id.get(..16).unwrap_or(id).to_owned(),
                |member| member.name.clone(),
            )
    };
    let mut lines = Vec::new();
    if part == Part::All {
        lines.push(format!("Fleet {}", view.fleet));
        lines.push(format!(
            "{} members, {} hosts, {} removed",
            view.members.len(),
            view.hosts.len(),
            view.removed
        ));
        for id in &view.suspended {
            lines.push(format!(
                "Cut off until its removal's history arrives: {}",
                name(id)
            ));
        }
        for id in &view.equivocating {
            lines.push(format!("Signed in two places: {}", name(id)));
        }
    }
    if part != Part::Hosts {
        if part == Part::All {
            lines.push("Members:".to_owned());
        }
        for member in &view.members {
            let kind = match member.kind {
                Kind::Computer => "computer",
                Kind::Phone => "phone",
            };
            let by = member.admitted_by.as_deref().map_or_else(
                || "founder".to_owned(),
                |by| format!("admitted by {}", name(by)),
            );
            let me = if member.id == view.me {
                "  (this computer)"
            } else {
                ""
            };
            lines.push(format!("{}  {}  {kind}  {by}{me}", member.id, member.name));
        }
    }
    if part != Part::Members {
        if part == Part::All {
            lines.push("Hosts:".to_owned());
        }
        for host in &view.hosts {
            let kind = match host.kind {
                HostKind::Member => "member",
                HostKind::Sheprd => "sheprd",
                HostKind::Agent => "agent",
            };
            let mut line = format!(
                "{}  {}  {kind}  added by {}  {} allowed",
                host.id,
                host.name,
                name(&host.added_by),
                host.grants.len()
            );
            if let Some(target) = &host.target {
                line.push_str(&format!("  ssh {target}"));
            }
            if host.held {
                line.push_str("  held: two members added it with different keys");
            }
            lines.push(line);
        }
    }
    lines.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
