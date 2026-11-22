//! A pull request opened from elsewhere, such as a chat's rail, shows its
//! row in the list, selected, whatever the search and filters hide.
use super::*;
use crate::orchestrator::{
    PullRequest, Request, Timing,
    service::PullRequestLookup,
    view::{RepoSpec, repo::Repo, rows::Tab},
};
use std::sync::Arc;

const URL: &str = "https://github.com/penso/app/pull/8";

fn pull_request(number: u64, title: &str) -> Item {
    let mut pr = item(
        Provider::Github,
        &format!("pr/{number}"),
        title,
        "2026-10-04T00:00:00Z",
    );
    pr.url = Some(format!("https://github.com/penso/app/pull/{number}"));
    pr.pull_request = Some(PullRequest {
        number,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: format!("topic-{number}"),
        base_sha: String::new(),
        head_sha: String::new(),
        head_repository: None,
    });
    pr
}

#[gpui::test]
fn opening_a_hidden_pull_request_lists_and_selects_its_row(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let pr = pull_request(8, "Menus");
            let key = pr.key.canonical();
            view.repo_mut().snapshot.items = Arc::new(vec![pull_request(7, "Other"), pr]);
            // On another list, searching for something else, Mine only.
            view.show_list(Tab::Issues, cx);
            view.query = "nothing like it".into();
            view.yours.authored = true;
            view.refresh_rows();

            assert!(view.open_pull_request_at(&URL.to_uppercase(), cx));
            assert_eq!(view.tab, Tab::PullRequests);
            assert_eq!(
                view.keys(),
                std::slice::from_ref(&key),
                "only the opened row is added"
            );
            assert_eq!(view.selected.as_deref(), Some(key.as_str()));
            assert_eq!(view.detail.as_ref().map(|detail| &detail.key), Some(&key));

            // A later sync keeps it while it stays open.
            view.refresh_rows();
            assert_eq!(view.keys(), std::slice::from_ref(&key));

            // Closed, the filters hide it again.
            view.detail = None;
            view.refresh_rows();
            assert!(view.keys().is_empty());
        })
    });
}

#[gpui::test]
fn a_pull_request_the_section_does_not_list_is_not_opened(cx: &mut gpui::TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.repo_mut().snapshot.items = Arc::new(vec![pull_request(7, "Other")]);
            view.show_list(Tab::Issues, cx);
            assert!(!view.open_pull_request_at(URL, cx));
            assert_eq!(view.tab, Tab::Issues);
            assert!(view.detail.is_none());
        })
    });
}

#[gpui::test]
fn the_page_reads_the_checkout_the_revealed_row_shows(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let pr = pull_request(8, "Menus");
            let key = pr.key.canonical();
            let request = Request {
                target: sheprd_client::ConnectTarget::Local,
                checkout: dir.path().join("other").to_string_lossy().into_owned(),
                workspace_id: "other".into(),
                token: None,
                data_root: Some(dir.path().join("other-data")),
                azure_store: Default::default(),
                gitlab_store: Default::default(),
                timing: Timing::default(),
            };
            view.repos.push(Repo::start(RepoSpec {
                key: "other".into(),
                host: "9950x3d.local".into(),
                request,
            }));
            for repo in &mut view.repos {
                repo.snapshot.items = Arc::new(vec![pr.clone()]);
            }
            // The pill lists only the second checkout, which shows it.
            view.show_list(Tab::PullRequests, cx);
            view.projects = std::collections::HashSet::from(["other".to_owned()]);
            view.refresh_rows();

            assert!(view.open_pull_request_at(URL, cx));
            assert_eq!(view.keys(), std::slice::from_ref(&key));
            assert_eq!(view.repo_of(&key), Some(1), "the picked checkout's row");
            assert_eq!(view.active, 1, "the page reads the row's checkout");
            assert_eq!(view.twins(&key), 0);

            // Hidden by the search too, it stays the picked checkout's.
            view.query = "nothing like it".into();
            view.refresh_rows();
            assert_eq!(view.keys(), std::slice::from_ref(&key));
            assert_eq!(view.repo_of(&key), Some(1));

            // Picking the other checkout moves the row there, and the page
            // follows it, reading the pull request that checkout looked up.
            let mut looked_up = crate::pull_request::fixture().unwrap();
            looked_up.number = 8;
            view.repos[0].snapshot.pull_request = Some(PullRequestLookup {
                number: 8,
                result: Arc::new(Ok(Some(looked_up))),
            });
            assert!(
                view.pr.target().is_none(),
                "the first checkout looked up nothing"
            );
            view.projects = std::collections::HashSet::from(["test".to_owned()]);
            view.refresh_rows();
            assert_eq!(view.repo_of(&key), Some(0));
            assert_eq!(view.active, 0);
            assert!(view.pr.target().is_some());
        })
    });
}
