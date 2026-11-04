//! Linear cards dropped in another lane, or another place in their own. A
//! card's place is Linear's `sortOrder`: midway between its new neighbours'.
//! The move goes to the worker that lists the issue, which alone holds the
//! API key, and the card shows in its new place at once. It shows there
//! until that worker's snapshot echoes the move's sequence
//! (`Snapshot::linear_moved`): moved, the snapshot holds the state and
//! place Linear answered with; refused, the issue where it was, beside the
//! notice saying why.

use super::{
    OrchestratorView,
    board::{CardDrag, sort_order_of, status_of},
};
use crate::orchestrator::{Error, Notice, SortOrder, Status, Tracker, linear::Move};
use gpui::Context;
use std::{collections::HashMap, sync::Arc};

/// A move asked for and not yet done.
#[derive(Clone, Debug)]
struct Moving {
    /// The repository whose worker was asked, by key.
    repo: String,
    sequence: u64,
    to: Status,
    sort_order: Option<SortOrder>,
}

/// The moves under way, by item key.
#[derive(Debug, Default)]
pub(super) struct Moves {
    pending: HashMap<String, Moving>,
    /// Moves asked for so far, which numbers the next.
    sequence: u64,
}

/// The state `tracker`'s item lands in when dropped in the lane of the
/// state named `to`, and, when that is another state, the id that moves it
/// there: its team's.
fn target(tracker: &Tracker, to: &str) -> Option<(Status, Option<String>)> {
    if let Some(status) = tracker.status.as_ref().filter(|status| status.name == to) {
        return Some((status.clone(), None));
    }
    let state = tracker.states.iter().find(|state| state.name == to)?;
    Some((state.clone(), Some(state.id.clone()?)))
}

/// The place of `card` dropped among `lane`'s cards, given top to bottom
/// with their places: right above the card `below`, else last. Midway
/// between its new neighbours, or one past the only one at an end; `None`
/// alone in the lane, or beside a card without a place.
pub(super) fn sort_order_at(
    lane: &[(String, Option<SortOrder>)],
    card: &str,
    below: Option<&str>,
) -> Option<SortOrder> {
    let others: Vec<&(String, Option<SortOrder>)> =
        lane.iter().filter(|(key, _)| key != card).collect();
    let at = below
        .and_then(|below| others.iter().position(|(key, _)| key == below))
        .unwrap_or(others.len());
    let above = at.checked_sub(1).map(|above| others[above].1);
    let below = others.get(at).map(|(_, order)| *order);
    match (above, below) {
        (Some(Some(SortOrder(above))), Some(Some(SortOrder(below)))) => {
            Some(SortOrder((above + below) / 2.))
        }
        (Some(Some(SortOrder(above))), None) => Some(SortOrder(above + 1.)),
        (None, Some(Some(SortOrder(below)))) => Some(SortOrder(below - 1.)),
        _ => None,
    }
}

impl OrchestratorView {
    /// Moves the issue `drag` carries into the lane of the state named
    /// `to`, right above the card `below`, else last, as dropping it there
    /// asks.
    pub(super) fn place_card(
        &mut self,
        drag: &CardDrag,
        to: &str,
        below: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(repo) = self.repo_of(&drag.key) else {
            return;
        };
        let lane: Vec<(String, Option<SortOrder>)> = self
            .board_lanes()
            .into_iter()
            .find(|(status, _)| status.as_ref().is_some_and(|status| status.name == to))
            .map(|(_, rows)| {
                rows.iter()
                    .map(|row| {
                        let item = self.row_item(row);
                        (item.key.canonical(), sort_order_of(item))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let sort_order = sort_order_at(&lane, &drag.key, below);
        let Some((item, (target, state))) = self.repos[repo]
            .item(&drag.key)
            .and_then(|item| Some((item, target(item.tracker.as_ref()?, to)?)))
        else {
            return;
        };
        // Its own place in its own lane, or a lane it has no place in yet.
        if state.is_none() && sort_order.is_none() {
            return;
        }
        let sequence = self.moves.sequence + 1;
        let change = Move {
            // Linear also takes the identifier, should the UUID be missing.
            issue: item
                .tracker
                .as_ref()
                .and_then(|tracker| tracker.id.clone())
                .unwrap_or_else(|| item.key.native_id.clone()),
            identifier: item.identifier.clone(),
            state,
            sort_order,
            sequence,
        };
        let identifier = change.identifier.clone();
        let sent = self.repos[repo]
            .service
            .as_ref()
            .is_some_and(|service| service.move_linear_issue(change));
        if !sent {
            self.notice = Some(Notice {
                outcome: Err(Arc::new(Error::LinearMove {
                    issue: identifier,
                    source: Arc::new(Error::Busy),
                })),
            });
            cx.notify();
            return;
        }
        self.moves.sequence = sequence;
        self.moves.pending.insert(
            drag.key.clone(),
            Moving {
                repo: self.repos[repo].key.clone(),
                sequence,
                to: target,
                sort_order,
            },
        );
        self.show_moves();
        self.refresh_rows();
        cx.notify();
    }

    /// Forgets the moves their worker has done, or whose repository is no
    /// longer listed, then shows the rest over the snapshots just read.
    pub(super) fn settle_moves(&mut self) {
        if self.moves.pending.is_empty() {
            return;
        }
        let repos = &self.repos;
        self.moves.pending.retain(|_, moving| {
            repos
                .iter()
                .find(|repo| repo.key == moving.repo)
                .is_some_and(|repo| repo.snapshot.linear_moved < moving.sequence)
        });
        self.show_moves();
    }

    /// Shows each move under way in its repository's listing, the item in
    /// the state and place it is moving to.
    fn show_moves(&mut self) {
        for (key, moving) in &self.moves.pending {
            let Some(repo) = self.repos.iter_mut().find(|repo| repo.key == moving.repo) else {
                continue;
            };
            let Some(index) = repo
                .snapshot
                .items
                .iter()
                .position(|item| item.key.canonical() == *key)
            else {
                continue;
            };
            let item = &repo.snapshot.items[index];
            if status_of(item) == Some(&moving.to)
                && moving
                    .sort_order
                    .is_none_or(|order| sort_order_of(item) == Some(order))
            {
                continue;
            }
            // The worker shares the listing; it is copied once, here.
            let item = &mut Arc::make_mut(&mut repo.snapshot.items)[index];
            if let Some(tracker) = item.tracker.as_mut() {
                item.state = moving.to.name.to_lowercase();
                tracker.status = Some(moving.to.clone());
                tracker.sort_order = moving.sort_order.or(tracker.sort_order);
            }
        }
    }

    /// Whether a move of `key` is under way, for tests.
    #[cfg(test)]
    pub(crate) fn moving(&self, key: &str) -> bool {
        self.moves.pending.contains_key(key)
    }
}
