//! The `pull-requests` and `pull-requests-before` cases: an agent that
//! pushed a branch, opened a pull request, and merged and commented on
//! others, with the rail open. The first lists them in the rail's Pull
//! requests section; the second is the same chat without that list.
//! With the `mockup` feature, the app's pull request lookups know three of
//! them, so their rows show titles and badge colors; the fourth shows as
//! one no lookup knows.

use sheprd_chat_protocol::{
    Entry, EntryId, EntryKind, PullRequest, PullRequestAction, Tool, ToolContent, ToolKind,
    ToolStatus, Transcript,
};

const REPO: &str = "sheprd-labs/sheprd"; // i18n: data

fn shell(id: u64, call: &str, title: &str, output: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: call.into(),
            content: vec![ToolContent::Text {
                text: output.into(),
            }],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: title.into(),
        }),
    }
}

/// The conversation, and with `listed` the pull requests it worked on.
pub(super) fn apply(transcript: &mut Transcript, listed: bool) {
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Ship the rail section, merge the follow-up once CI passes, and answer the review on the docs PR.".into(), // i18n: data
                files: 0,
                note: None,
            },
        },
        shell(2, "push", "`git push -u origin feat/chat-rail-pull-requests`", "To github.com:sheprd-labs/sheprd.git\n * [new branch] feat/chat-rail-pull-requests"), // i18n: data
        shell(3, "create", "`gh pr create --fill`", "https://github.com/sheprd-labs/sheprd/pull/1455"), // i18n: data
        shell(4, "merge", "`gh pr merge 1452 --squash --repo sheprd-labs/sheprd`", "✓ Squashed and merged pull request sheprd-labs/sheprd#1452"), // i18n: data
        shell(5, "comment", "`gh pr comment 1440 --body-file reply.md`", "https://github.com/sheprd-labs/sheprd/pull/1440#issuecomment-1"), // i18n: data
        Entry {
            id: EntryId(6),
            kind: EntryKind::Agent {
                text: "Opened #1455, merged #1452, and answered the review on #1440. The rail lists every pull request I touched in this chat.".into(), // i18n: data
            },
        },
    ];
    if !listed {
        return;
    }
    let pull =
        |number, actions: &[PullRequestAction]| PullRequest::new(REPO, number, actions.to_vec());
    transcript.pull_requests = [
        pull(1436, &[PullRequestAction::Reviewed]),
        pull(1440, &[PullRequestAction::Commented]),
        pull(1452, &[PullRequestAction::Merged]),
        pull(
            1455,
            &[PullRequestAction::Pushed, PullRequestAction::Created],
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
}

/// What the app's lookups know of three of them: one open with a failed
/// check, one merged, one a draft.
#[cfg(feature = "mockup")]
pub(super) fn seed(cache: &mut crate::pull_request::Cache) {
    use crate::pull_request::{Input, State, fixture};
    const OPEN: &str = "Chat rail: list the pull requests an agent works on"; // i18n: data
    const MERGED: &str = "Keep the composer's draft across reconnects"; // i18n: data
    const DRAFT: &str = "Document the Pull requests section"; // i18n: data
    let known = [
        (1455, OPEN, State::Open, false),
        (1452, MERGED, State::Merged, false),
        (1440, DRAFT, State::Open, true),
    ];
    let now = std::time::Instant::now();
    for (number, title, state, draft) in known {
        let Ok(mut pr) = fixture() else {
            return;
        };
        pr.number = number;
        pr.url = PullRequest::url_of(REPO, number);
        pr.title = title.into();
        pr.state = state;
        pr.is_draft = draft;
        let input = Input {
            checkout: None,
            repo_key: Some("/preview/.git".into()),
            branch: format!("preview-{number}"),
        };
        cache.seed(input, pr, now);
    }
}
