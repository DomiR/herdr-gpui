use crate::control::protocol::doc::{LinkMode, LinkStatus};

pub(super) fn status_message(status: &LinkStatus) -> String {
    let mut text = match status.mode {
        LinkMode::Off => "off".to_owned(),
        LinkMode::On => "on".to_owned(),
        LinkMode::Paused => "paused".to_owned(),
        LinkMode::Restoring => "restoring".to_owned(),
        LinkMode::Starting => "starting".to_owned(),
    };
    if matches!(status.mode, LinkMode::On | LinkMode::Paused) {
        let mut details = String::new();
        if let Some(agent) = &status.agent {
            details.push_str(&format!("agent {agent}"));
        }
        if let Some(pane) = &status.pane {
            if !details.is_empty() {
                details.push_str(" in ");
            }
            details.push_str(&format!("pane {pane}"));
        }
        if let Some(acked) = status.acked {
            if !details.is_empty() {
                details.push_str(", ");
            }
            details.push_str(&format!("pushed through version {acked}"));
        }
        if !details.is_empty() {
            text.push_str(": ");
            text.push_str(&details);
        }
    }
    if let Some(pending) = &status.pending {
        let state = if pending.in_flight {
            "being sent"
        } else {
            "pending"
        };
        text.push_str(&format!(
            ", versions {} to {} {state}",
            pending.from, pending.to
        ));
    }
    text
}
