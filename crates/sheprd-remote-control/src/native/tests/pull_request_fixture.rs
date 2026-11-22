//! The pull requests a chat sends a phone, written as the fixture its
//! Swift tests decode, so the phone's activity panel lists what the
//! desktop rail does.

use super::fixtures::{check, cursor, dir};
use super::*;
use sheprd_chat_protocol::{Change, PullRequest, PullRequestAction};
use wire::Step;

const NAME: &str = "chat_pull_requests";

fn message() -> ServerMessage {
    let step = |seq, repo, number, actions| Step {
        cursor: cursor(seq),
        change: Change::PullRequest {
            pull_request: PullRequest::new(repo, number, actions).unwrap(),
        },
    };
    ServerMessage::ChatChanges {
        pane: "p1".into(),
        changes: vec![
            step(
                45,
                "sheprd-labs/sheprd",
                1490,
                vec![PullRequestAction::Created],
            ),
            step(46, "sheprd-labs/sheprd", 1489, Vec::new()),
            step(
                47,
                "Sheprd-Labs/Sheprd",
                1490,
                vec![PullRequestAction::Pushed, PullRequestAction::Created],
            ),
        ],
    }
}

#[test]
fn pull_request_changes_match_the_swift_fixture() {
    check(NAME, &message());
}

#[test]
fn the_pull_request_fixture_reads_back_as_what_wrote_it() {
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        return;
    }
    let text = std::fs::read_to_string(dir().join(format!("{NAME}.json"))).unwrap();
    let message: ServerMessage = serde_json::from_str(&text).unwrap();
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&message).unwrap()),
        text
    );
}
