//! `pr`, `pr checks`, `pr comments`, and `pr open`, read from the window's
//! pull request cache. The cache is filled for the selected host's
//! workspaces only, and the review conversation is loaded only for the
//! focused branch, whose pull request the window's actions follow.
use super::{Place, Watches, socket, window_for};
use crate::{
    SheprdWindow,
    browser::WebUrl,
    control::protocol::{
        Caller, ErrorCode, Response,
        hosts::{
            AccountQuota, COMMENTS_WAIT_SECONDS, CheckInfo, CommentInfo, LayerInfo, PrInfo,
            QueueInfo, QueueOp, Reply, StackInfo,
        },
    },
    menu::PrChange,
    pr_actions::{Comment, Target},
    pull_request::{MergeState, PullRequest, ReviewDecision, State, Verdict, workspace_input},
};
use gpui::{App, Context, Window};
use std::time::Duration;

/// What the cache knows of the caller's branch.
enum Lookup {
    Found(Box<PullRequest>),
    /// No pull request, with why when that is not simply the answer.
    Missing(Option<String>),
}

fn lookup(view: &SheprdWindow, caller: &Caller) -> Result<(Place, Lookup), Response> {
    let place = Place::of(view, caller)?;
    if !place.selected(view) {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Workspace {} is on {}, and Sheprd looks up pull requests for the \
                 window's selected host only; run `sheprd host select {}` first",
                place.workspace, place.label, place.endpoint
            ),
        ));
    }
    if !view.pr_signed_in() {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            "Sign in to GitHub in Sheprd first",
        ));
    }
    let Some(origin) = view.pr_origin() else {
        return Err(Response::error(
            ErrorCode::Unsupported,
            "Sheprd looks up pull requests only for this machine's own daemon and saved SSH hosts",
        ));
    };
    // A workspace the app keeps for Herdr 2 is looked up by the checkout
    // Git names for its folder, as the window prefetches it.
    let kept = view
        .kept_checkout_inputs()
        .into_iter()
        .find(|(id, _)| *id == place.workspace)
        .map(|(_, input)| input);
    let input = match kept {
        Some(input) => input,
        None if view.is_kept_workspace(&place.workspace) => {
            return Err(Response::error(
                ErrorCode::InvalidRequest,
                format!(
                    "Workspace {} is not a Git checkout on a branch, as far as Sheprd knows yet",
                    place.workspace
                ),
            ));
        }
        None => {
            let workspace = place.workspace(view).ok_or_else(|| {
                Response::error(
                    ErrorCode::WorkspaceNotFound,
                    format!("Workspace {} closed", place.workspace),
                )
            })?;
            workspace_input(workspace, &origin)
                .map_err(|error| Response::error(ErrorCode::InvalidRequest, error.to_string()))?
        }
    };
    let found = match view.menu.pr_cache.lookup(&input) {
        Some((Some(pr), _)) => Lookup::Found(Box::new(pr)),
        Some((None, message)) => Lookup::Missing(message),
        None => Lookup::Missing(Some(
            "Sheprd is still looking up this branch's pull request; try again shortly".into(),
        )),
    };
    // A private repository's lookup fails until the App is installed and
    // approved, which only the user can do; say so beside GitHub's answer.
    let found = match found {
        Lookup::Missing(note)
            if view
                .pr_profile()
                .is_some_and(|profile| profile.access.needs_attention()) =>
        {
            let hint = "Sheprd's GitHub App needs installing or its permissions approved; the user can do it from the GitHub page in Sheprd";
            Lookup::Missing(Some(match note {
                Some(note) => format!("{note}. {hint}"),
                None => hint.to_owned(),
            }))
        }
        found => found,
    };
    Ok((place, found))
}

fn missing(note: Option<String>) -> Response {
    Response::Host {
        reply: Reply::Pr { pr: None, note },
    }
}

pub(super) fn status(view: &SheprdWindow, caller: &Caller) -> Response {
    match lookup(view, caller) {
        Ok((_, Lookup::Found(pr))) => Response::Host {
            reply: Reply::Pr {
                pr: Some(Box::new(pr_info(&pr))),
                note: None,
            },
        },
        Ok((_, Lookup::Missing(note))) => missing(note),
        Err(response) => response,
    }
}

pub(super) fn create(
    view: &mut SheprdWindow,
    caller: &Caller,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let place = match Place::of(view, caller) {
        Ok(place) => place,
        Err(response) => return response,
    };
    if !place.focused(view) {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Focus workspace {} before asking to create its pull request",
                place.workspace
            ),
        );
    }
    if view.main_tokens().is_none() {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Sign in to GitHub in Sheprd first",
        );
    }
    let Some(input) = view.git_input() else {
        return Response::error(
            ErrorCode::Unsupported,
            "Pull request creation needs a local Git checkout on a branch",
        );
    };
    if let Some(terminal) = caller.terminal_id.as_deref()
        && !view.shown_core_pane(cx).is_some_and(|(tab, focused)| {
            focused.as_str() == terminal
                && view.core_pane_git_input(tab, &focused).as_ref() == Some(&input)
        })
    {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Focus terminal {terminal} and wait for its checkout before asking to create its pull request"
            ),
        );
    }
    if view.git.tracked() != Some(&input) {
        return Response::error(
            ErrorCode::InvalidRequest,
            "Sheprd is still checking this checkout; try again shortly",
        );
    }
    if let Ok((_, Lookup::Found(pr))) = lookup(view, caller)
        && pr.state == State::Open
    {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "#{} is already open for this branch; use `sheprd pr open`",
                pr.number
            ),
        );
    }
    if !view.ask_pr_create(window, cx) {
        return Response::error(
            ErrorCode::WindowBusy,
            "A menu, dialog, or Git operation is open in the window; close it or wait first",
        );
    }
    super::super::work::bring_forward(window, cx);
    Response::Done {
        message: format!(
            "Opened Create pull request for {} in Sheprd; the user confirms with a click. Nothing has been pushed or created",
            input.branch
        ),
    }
}

pub(super) fn quota(view: &SheprdWindow) -> Response {
    let now = std::time::Instant::now();
    let mut accounts: Vec<AccountQuota> = Vec::new();
    for auth in view.github_auths().take(128) {
        let Some(profile) = &auth.profile else {
            continue;
        };
        let observed = AccountQuota {
            login: profile.login.clone(),
            quota: auth.quota(),
            retry_after_seconds: view
                .github_rate_limit_wait(auth, now)
                .map(|wait| wait.as_secs().max(1)),
        };
        if let Some(account) = accounts
            .iter_mut()
            .find(|account| account.login == profile.login)
        {
            // One login can have several credentials. Prefer a known observation
            // from the latest quota window, then the furthest consumption within
            // that window. Keep the whole observation; never sum shared usage.
            account.quota = account
                .quota
                .into_iter()
                .chain(observed.quota)
                .max_by_key(|quota| {
                    (
                        quota.reset,
                        quota.used,
                        std::cmp::Reverse(quota.remaining),
                        quota.limit,
                    )
                });
            account.retry_after_seconds = account
                .retry_after_seconds
                .max(observed.retry_after_seconds);
        } else {
            accounts.push(observed);
        }
    }
    Response::Host {
        reply: Reply::PrQuota { accounts },
    }
}

pub(super) fn pr_info(pr: &PullRequest) -> PrInfo {
    PrInfo {
        number: pr.number,
        url: pr.url.clone(),
        title: pr.title.clone(),
        state: pr.lifecycle().to_owned(),
        draft: pr.is_draft,
        head: pr.head_ref_name.clone(),
        base: pr.base_ref_name.clone(),
        additions: pr.additions,
        deletions: pr.deletions,
        changed_files: pr.changed_files,
        review: (pr.review_decision != ReviewDecision::None).then(|| pr.review().to_owned()),
        merge_status: (pr.merge_state_status != MergeState::Unknown)
            .then(|| pr.merge_status().to_owned()),
        checks: pr
            .check_list()
            .map(|(name, outcome)| CheckInfo {
                name: name.to_owned(),
                outcome: outcome.to_string(),
            })
            .collect(),
        stack: stack_info(pr),
        queue: pr.queue.map(|queue| QueueInfo {
            position: queue.position,
            state: queue.state.key().to_owned(),
            eta_seconds: queue.eta,
        }),
    }
}

fn stack_info(pr: &PullRequest) -> Option<StackInfo> {
    let (stack, position) = pr.stack()?;
    let (verdict, ready, blocked) = match stack.verdict(position) {
        Verdict::Merged => ("merged", None, None),
        Verdict::Ready { count } => ("ready", Some(count), None),
        Verdict::Unread => ("unread", None, None),
        Verdict::Blocked { number, blocker } => ("blocked", None, Some((number, blocker))),
    };
    Some(StackInfo {
        number: stack.number,
        base: stack.base.clone(),
        size: stack.size,
        position,
        verdict: verdict.to_owned(),
        ready,
        blocked_by: blocked.map(|(number, _)| number),
        reason: blocked.map(|(_, blocker)| blocker.key().to_owned()),
        layers: stack
            .layers
            .iter()
            .map(|layer| LayerInfo {
                position: layer.position,
                number: layer.number,
                title: layer.title.clone(),
                status: layer.status_key().to_owned(),
            })
            .collect(),
    })
}

pub(super) fn comment_info(comment: &Comment) -> CommentInfo {
    CommentInfo {
        author: comment.author.clone(),
        kind: comment.kind.label(),
        created_at: comment.created_at.clone(),
        body: comment.body.clone(),
    }
}

/// Starts loading the conversation of the caller's pull request, when the
/// window's actions already follow it, and returns it. A second follower
/// would race the window's own, so another workspace is refused.
fn start_comments(view: &mut SheprdWindow, caller: &Caller) -> Result<Target, Response> {
    let (place, wanted) = {
        let (place, found) = lookup(view, caller)?;
        let pr = match found {
            Lookup::Found(pr) => pr,
            Lookup::Missing(note) => return Err(missing(note)),
        };
        (place, Target::for_reading(&pr).ok())
    };
    let followed = view
        .pr_actions
        .target()
        .filter(|target| {
            wanted
                .as_ref()
                .is_some_and(|wanted| target.same_pull_request(wanted))
        })
        .cloned();
    let Some(target) = followed.filter(|_| place.focused(view)) else {
        return Err(Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Comments load for the focused workspace's open pull request; \
                 focus workspace {} first",
                place.workspace
            ),
        ));
    };
    let token = view.pr_token().ok_or_else(|| {
        Response::error(
            ErrorCode::InvalidRequest,
            "Sign in to GitHub in Sheprd first",
        )
    })?;
    view.pr_actions
        .load_comments(token)
        .map_err(|error| Response::error(ErrorCode::Failed, error.to_string()))?;
    Ok(target)
}

/// The answer once the conversation of `wanted` loaded or failed; `None`
/// while it is still loading.
pub(super) fn loaded(view: &SheprdWindow, wanted: &Target) -> Option<Response> {
    let actions = &view.pr_actions;
    if !actions
        .target()
        .is_some_and(|target| target.same_pull_request(wanted))
    {
        return Some(Response::error(
            ErrorCode::Failed,
            "The focused pull request changed while its comments loaded",
        ));
    }
    if actions.loading() {
        return None;
    }
    Some(match (actions.comments_error(), actions.comments()) {
        (Some(error), _) => Response::error(ErrorCode::Failed, error),
        (None, Some(comments)) => Response::Host {
            reply: Reply::Comments {
                comments: comments.iter().map(comment_info).collect(),
            },
        },
        (None, None) => Response::error(ErrorCode::Failed, "The comments did not load"),
    })
}

pub(super) fn comments(
    caller: Caller,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let handle = match window_for(&caller, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let started = handle
        .update(cx, |view, _, _| start_comments(view, &caller))
        .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")));
    let wanted = match started {
        Ok(target) => target,
        Err(response) => return incoming.respond(response),
    };
    watches.add(
        incoming,
        Duration::from_secs(COMMENTS_WAIT_SECONDS),
        move |cx| {
            handle
                .read_with(cx, |view, _| loaded(view, &wanted))
                .unwrap_or_else(|_| Some(Response::error(ErrorCode::NoWindow, "The window closed")))
        },
        Response::error(ErrorCode::Timeout, "GitHub did not answer in time"),
        cx,
    );
}

pub(super) fn open(
    view: &mut SheprdWindow,
    caller: &Caller,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let (place, url) = match lookup(view, caller) {
        Ok((place, Lookup::Found(pr))) => (place, pr.url.clone()),
        Ok((_, Lookup::Missing(note))) => return missing(note),
        Err(response) => return response,
    };
    let url = match WebUrl::try_from(url.as_str()) {
        Ok(url) => url,
        Err(error) => return Response::error(ErrorCode::InvalidUrl, error.to_string()),
    };
    let how = view.open_workspace_page(&place.endpoint, &place.workspace, url, window, cx);
    super::page_answer(Some(how), || {
        format!("Opened the pull request in workspace {}", place.workspace)
    })
}

/// `pr merge` and `pr queue`: opens the Git popup's confirmation for the
/// caller's pull request, which must be the focused workspace's, as the
/// popup follows. Nothing is merged or queued until the user confirms.
pub(super) fn ask(
    view: &mut SheprdWindow,
    caller: &Caller,
    change: PrChange,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let (place, pr) = match lookup(view, caller) {
        Ok((place, Lookup::Found(pr))) => (place, pr),
        Ok((_, Lookup::Missing(note))) => return missing(note),
        Err(response) => return response,
    };
    // What the pull request itself rules out comes first: focusing a
    // workspace cannot open a closed one.
    if let Some(reason) = refusal(&pr, change) {
        return Response::error(ErrorCode::InvalidRequest, reason);
    }
    let followed = Target::try_from(&*pr).ok().is_some_and(|wanted| {
        view.pr_actions
            .target()
            .is_some_and(|target| target.same_pull_request(&wanted))
    });
    if !followed || !place.focused(view) {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!(
                "Merging and queueing confirm in the Git popup, which follows the focused \
                 workspace's open pull request; focus workspace {} first",
                place.workspace
            ),
        );
    }
    if !view.ask_pr_change(change, window, cx) {
        return Response::error(
            ErrorCode::WindowBusy,
            "A menu or dialog is open in the window; close it first",
        );
    }
    super::super::work::bring_forward(window, cx);
    let what = match change {
        PrChange::Merge => "merge dialog",
        PrChange::Queue(QueueOp::Add) => "merge queue confirmation",
        PrChange::Queue(QueueOp::Remove) => "confirmation to leave the merge queue",
    };
    Response::Done {
        message: format!(
            "Opened the {what} for #{} in Sheprd; the user confirms there",
            pr.number
        ),
    }
}

/// Why `change` cannot be offered for `pr`, as the command says it.
pub(super) fn refusal(pr: &PullRequest, change: PrChange) -> Option<String> {
    let number = pr.number;
    if pr.state != State::Open {
        return Some(format!("#{number} is not open"));
    }
    if pr.is_draft {
        return Some(format!(
            "#{number} is a draft; mark it ready for review first"
        ));
    }
    match change {
        PrChange::Merge if pr.queue.is_some() => Some(format!(
            "#{number} is in the merge queue, which merges it; `sheprd pr queue remove` takes it out"
        )),
        PrChange::Merge if pr.merge_methods.is_empty() => {
            Some("The repository allows no merge method".into())
        }
        PrChange::Merge => match pr.stack().map(|(stack, position)| stack.verdict(position)) {
            Some(Verdict::Blocked {
                number: blocking,
                blocker,
            }) => Some(format!(
                "The stack cannot merge yet: #{blocking} is {}",
                blocker.key().replace('_', " ")
            )),
            Some(Verdict::Unread) => Some(unread(number)),
            _ => None,
        },
        PrChange::Queue(QueueOp::Add) if !pr.stack_known() => Some(unread(number)),
        PrChange::Queue(QueueOp::Add) if !pr.merge_queue => {
            Some(format!("#{number}'s base branch has no merge queue"))
        }
        PrChange::Queue(QueueOp::Add) if pr.queue.is_some() => {
            Some(format!("#{number} is already in the merge queue"))
        }
        PrChange::Queue(QueueOp::Remove) if pr.queue.is_none() => {
            Some(format!("#{number} is not in the merge queue"))
        }
        PrChange::Queue(_) => None,
    }
}

/// Why a stack with a layer this reader could not see is not landed from
/// here: the confirmation could not name everything GitHub would land.
fn unread(number: u64) -> String {
    format!(
        "Not every pull request below #{number} in its stack could be read, so what would land \
         is not known; check the stack on GitHub"
    )
}
