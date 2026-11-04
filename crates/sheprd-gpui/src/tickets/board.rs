//! What the native board shows, built from herdr-dog's answers without
//! drawing anything: the lanes with their cards, the labels to filter by,
//! and which project shows.

use super::api::{Project, Status, Ticket};
use std::collections::{BTreeSet, HashMap};

/// A card on the board: its ticket, and the done and total counts of its
/// sub-tickets when it has any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Card<'a> {
    pub(crate) ticket: &'a Ticket,
    pub(crate) progress: Option<(usize, usize)>,
}

/// Each lane's cards in lane order, keeping tickets that carry a pressed
/// label (none pressed keeps every ticket). Progress counts every
/// sub-ticket, filtered or not, as herdr-dog's own board does.
pub(crate) fn lanes<'a>(
    tickets: &'a [Ticket],
    pressed: &BTreeSet<String>,
) -> [(Status, Vec<Card<'a>>); 6] {
    let mut progress: HashMap<i64, (usize, usize)> = HashMap::new();
    for ticket in tickets {
        if let Some(parent) = ticket.parent {
            let entry = progress.entry(parent).or_default();
            entry.0 += usize::from(ticket.status == Status::Done);
            entry.1 += 1;
        }
    }
    let mut shown: Vec<&Ticket> = tickets
        .iter()
        .filter(|ticket| pressed.is_empty() || ticket.labels.iter().any(|l| pressed.contains(l)))
        .collect();
    shown.sort_by(|a, b| a.sort_key.total_cmp(&b.sort_key));
    Status::LANES.map(|status| {
        let cards = shown
            .iter()
            .filter(|ticket| ticket.status == status)
            .map(|ticket| Card {
                ticket,
                progress: progress.get(&ticket.number).copied(),
            })
            .collect();
        (status, cards)
    })
}

/// Every label the project's tickets carry, sorted.
pub(crate) fn labels(tickets: &[Ticket]) -> Vec<&str> {
    let labels: BTreeSet<&str> = tickets
        .iter()
        .flat_map(|ticket| ticket.labels.iter().map(String::as_str))
        .collect();
    labels.into_iter().collect()
}

/// The project to show: the one picked while it still exists, else the
/// first by name, as herdr-dog's board falls back.
pub(crate) fn project(projects: &[Project], picked: Option<i64>) -> Option<i64> {
    picked
        .filter(|id| projects.iter().any(|project| project.id == *id))
        .or_else(|| {
            projects
                .iter()
                .min_by(|a, b| a.name.cmp(&b.name))
                .map(|project| project.id)
        })
}

/// A ticket's display reference, as `PREFIX-N`.
pub(crate) fn reference(projects: &[Project], ticket: &Ticket) -> String {
    match projects
        .iter()
        .find(|project| project.id == ticket.project_id)
    {
        Some(project) => format!("{}-{}", project.prefix, ticket.number),
        None => format!("#{}", ticket.number),
    }
}
