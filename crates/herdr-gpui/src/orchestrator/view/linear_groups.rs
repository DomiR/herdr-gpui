//! The Linear section's list grouped as Linear's own list is: by assignee,
//! the user first, then everyone else by name, and nobody last; inside each,
//! by workflow state in board order. A sub-issue stands on its own row, as
//! the row names its parent. Pure, so it is tested without a window.

use super::{
    board::status_of,
    rows::{Heading, ItemRow, Line, Sort},
};
use crate::orchestrator::{Item, Status};
use std::collections::{HashMap, HashSet};

/// A group's heading: its first row names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Group {
    pub(crate) row: ItemRow,
    /// The sum of its issues' estimates, when any has one.
    pub(crate) estimate: Option<u32>,
    /// Whether its rows show.
    pub(crate) open: bool,
}

/// Who an issue is assigned to, in the order the list shows them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Who<'a> {
    Me,
    /// Someone else: lowercased first, so case does not order them.
    Named(String, &'a str),
    Nobody,
}

fn who(item: &Item) -> Who<'_> {
    let Some(tracker) = &item.tracker else {
        return Who::Nobody;
    };
    match tracker.assignee.as_deref() {
        _ if tracker.mine => Who::Me,
        Some(name) => Who::Named(name.to_lowercase(), name),
        None => Who::Nobody,
    }
}

/// What `collapsed` holds for the assignee group of `item`. An item's key
/// starts with its provider, so a group's never names an item.
pub(crate) fn assignee_key(item: &Item) -> String {
    match who(item) {
        Who::Me => "group:me".into(),
        Who::Named(_, name) => format!("group:{name}"),
        Who::Nobody => "group:nobody".into(),
    }
}

/// What `collapsed` holds for the group of `item`'s assignee and state.
pub(crate) fn state_key(item: &Item) -> String {
    let state = status_of(item).map_or("", |status| status.name.as_str());
    format!("{} / {state}", assignee_key(item))
}

/// The sum of `items`' estimates, when any has one.
pub(crate) fn estimate<'a>(items: impl IntoIterator<Item = &'a Item>) -> Option<u32> {
    items
        .into_iter()
        .filter_map(|item| item.tracker.as_ref()?.estimate)
        .reduce(u32::saturating_add)
}

/// `rows` under a heading per assignee and, inside, per workflow state,
/// each group's rows in `sort`'s order. A group whose key `collapsed`
/// holds keeps its heading and hides what is under it.
pub(crate) fn lines<'a>(
    rows: &[ItemRow],
    item: impl Fn(&ItemRow) -> &'a Item,
    sort: Sort,
    collapsed: &HashSet<String>,
) -> Vec<Line> {
    // States are grouped by name, which teams share, in the board order of
    // the first one listed under each name.
    let mut states: Vec<&Status> = Vec::new();
    for row in rows {
        if let Some(status) = status_of(item(row))
            && !states.iter().any(|known| known.name == status.name)
        {
            states.push(status);
        }
    }
    states.sort_by(|a, b| a.order().cmp(&b.order()));
    let rank: HashMap<&str, usize> = states
        .iter()
        .enumerate()
        .map(|(at, status)| (status.name.as_str(), at))
        .collect();
    // Issues without a state go last.
    let state_rank = |item: &Item| {
        status_of(item)
            .and_then(|status| rank.get(status.name.as_str()).copied())
            .unwrap_or(usize::MAX)
    };
    let mut keyed: Vec<(Who<'a>, usize, ItemRow)> = rows
        .iter()
        .map(|row| {
            let item = item(row);
            let flat = ItemRow {
                depth: 0,
                children: None,
                ..*row
            };
            (who(item), state_rank(item), flat)
        })
        .collect();
    keyed.sort_by(|(a_who, a_state, a), (b_who, b_state, b)| {
        a_who
            .cmp(b_who)
            .then(a_state.cmp(b_state))
            .then_with(|| sort.compare(item(a), item(b)))
    });
    let group = |rows: &[(Who<'a>, usize, ItemRow)], key: String| Group {
        row: rows[0].2,
        estimate: estimate(rows.iter().map(|(_, _, row)| item(row))),
        open: !collapsed.contains(&key),
    };
    let mut lines = Vec::new();
    for person in keyed.chunk_by(|a, b| a.0 == b.0) {
        let assignee = group(person, assignee_key(item(&person[0].2)));
        lines.push(Line::Heading {
            heading: Heading::Assignee(assignee),
            count: person.len(),
        });
        if !assignee.open {
            continue;
        }
        for state in person.chunk_by(|a, b| a.1 == b.1) {
            let heading = group(state, state_key(item(&state[0].2)));
            lines.push(Line::Heading {
                heading: Heading::State(heading),
                count: state.len(),
            });
            if heading.open {
                lines.extend(state.iter().map(|(_, _, row)| Line::Row(*row)));
            }
        }
    }
    lines
}
