//! A review tab follows its checkout when the branch under it changes. A
//! checkout found through Git's worktree list is remembered with the tab, and
//! a worktree that moved to another branch is then read on that branch, as
//! its own HEAD says; the daemon refreshes a workspace's branch only while
//! its sidebar shows one. A review with no checkout remembered follows the
//! branch the daemon reports instead. Either way the tab is titled and saved
//! by the new branch, and the scope and queued notes stay.
use crate::{
    HerdrWindow,
    browser::{Location, ReviewCheckout, Store, TabId},
    pull_request::{Input, Origin, workspace_input},
    review::diff::{Loaded, Scope, load},
};
use gpui::Context;
use std::{path::Path, time::Instant};

/// Whether a review of `review` is of the checkout a workspace names as
/// `named`. A checkout remembered in the same repository is followed by its
/// own HEAD, never pulled back to a branch the daemon has not refreshed.
pub(crate) fn same_checkout(review: &Input, named: &Input) -> bool {
    match (&review.checkout, &named.checkout) {
        (Some(_), None) => review.repo_key.is_some() && review.repo_key == named.repo_key,
        _ => review == named,
    }
}

/// What a review load lists: `checkout`'s changes, or, when its remembered
/// checkout is now on another branch, that branch's. The checkout it read,
/// with the path Git's worktree list resolved remembered. Blocking.
pub(super) fn load_following(
    checkout: Input,
    scope: Scope,
    base: Option<&str>,
    base_hint: Option<&str>,
    whitespace: bool,
) -> (Input, crate::Result<Loaded>) {
    let first = load(&checkout, scope, base, base_hint, whitespace);
    let (checkout, result) = match first
        .as_ref()
        .err()
        .and_then(|error| moved(&checkout, error))
    {
        Some(next) => {
            let result = load(&next, scope, base, base_hint, whitespace);
            (next, result)
        }
        None => (checkout, first),
    };
    match &result {
        Ok(loaded) if checkout.checkout.is_none() && checkout.repo_key.is_some() => {
            let remembered = Input {
                checkout: Some(loaded.source.checkout.clone()),
                ..checkout.clone()
            };
            // A path the tabs file could not hold stays unremembered.
            match ReviewCheckout::try_from(&remembered) {
                Ok(_) => (remembered, result),
                Err(_) => (checkout, result),
            }
        }
        _ => (checkout, result),
    }
}

/// The checkout to read instead after `error`, for a remembered one: on the
/// branch its HEAD moved to, or, once it is gone or another repository, by
/// branch through the worktree list again. `local_checkout` verified the
/// repository before it compared the branch, so the branch read here is the
/// repository's own; a detached HEAD keeps the error.
fn moved(checkout: &Input, error: &crate::Error) -> Option<Input> {
    use crate::pull_request::Error as Lookup;
    let path = checkout.checkout.as_deref()?;
    checkout.repo_key.as_ref()?;
    let crate::Error::GitHub(herdr_github::Error::PullRequest(error)) = error else {
        return None;
    };
    let next = match error {
        Lookup::BranchChanged => Input {
            branch: branch_at(path)?,
            ..checkout.clone()
        },
        Lookup::RepositoryMismatch => Input {
            checkout: None,
            ..checkout.clone()
        },
        Lookup::Checkout if !Path::new(path).is_dir() => Input {
            checkout: None,
            ..checkout.clone()
        },
        _ => return None,
    };
    ReviewCheckout::try_from(&next).ok().map(|_| next)
}

/// The branch checked out at `path`, if HEAD is on one.
fn branch_at(path: &str) -> Option<String> {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    crate::git::git(
        path,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        "read the checkout's branch", // i18n: data
        deadline,
        &|| false,
    )
    .ok()
    .filter(|branch| !branch.is_empty())
}

impl HerdrWindow {
    /// Takes the checkout load `request` read as the review's, saving the
    /// tab by it, if that load is still the review's latest.
    pub(super) fn review_followed(
        &mut self,
        id: TabId,
        request: u64,
        checkout: Input,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self
            .reviews
            .get_mut(&id)
            .filter(|review| review.request == request && review.checkout != checkout)
        else {
            return;
        };
        let Ok(saved) = ReviewCheckout::try_from(&checkout) else {
            return;
        };
        review.checkout = checkout;
        save(id, saved, cx);
    }

    /// Points the focused workspace's review tabs at the checkout the daemon
    /// now reports for it, unless they remember their own; see
    /// [`same_checkout`]. Only once the Git chip follows that checkout too,
    /// which a load reads the status and the pull request's base from; until
    /// then a workspace just focused still has the last one's. Tabs of other
    /// workspaces follow when theirs is focused; a workspace gone from the
    /// snapshot leaves its tabs as they were.
    pub(super) fn follow_review_workspace(&mut self, cx: &mut Context<Self>) {
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let Some(input) = self
            .live
            .snapshot
            .as_deref()
            .and_then(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|shown| shown.workspace_id == workspace)
            })
            .and_then(|shown| workspace_input(shown, &Origin::Local).ok())
            .filter(|input| self.git.tracked() == Some(input))
        else {
            return;
        };
        let (Ok(checkout), Some(store)) =
            (ReviewCheckout::try_from(&input), cx.try_global::<Store>())
        else {
            return;
        };
        let moved: Vec<TabId> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| match &tab.location {
                Some(Location::Review { checkout: saved }) => {
                    !same_checkout(&Input::from(saved), &input)
                }
                _ => false,
            })
            .map(|tab| tab.id)
            .collect();
        for id in moved {
            save(id, checkout.clone(), cx);
            if let Some(review) = self.reviews.get_mut(&id) {
                review.checkout = input.clone();
                self.reload_review(id, cx);
            }
        }
    }
}

/// Saves tab `id` as the review of `checkout`, titled by its branch.
fn save(id: TabId, checkout: ReviewCheckout, cx: &mut Context<HerdrWindow>) {
    let location = Location::Review { checkout };
    let title = location.default_title();
    Store::update(cx, |store| store.visited(id, Some(location), Some(&title)));
}
