//! Linear syncs on an interval of its own, longer than the other sources',
//! and at once when the user asks.

use super::linear::{listing, worker};
use super::*;
use crate::{Repository, linear::QUERY, repo::Remote};

static PAGES: AtomicUsize = AtomicUsize::new(0);

fn counted(_: &SecretString, query: &str, _: Value) -> Result<Value> {
    if query == QUERY {
        PAGES.fetch_add(1, Ordering::SeqCst);
    }
    Ok(listing("acme", &["ENG-1"]))
}

fn github() -> SourceKey {
    SourceKey {
        provider: Provider::Github,
        host: "github.com".into(),
        repository: "acme/app".into(),
    }
}

/// Marks GitHub's state, so a sync that reads it shows.
fn mark(worker: &mut Worker) {
    worker.set_state(&github(), SyncState::Unsupported);
}

fn github_synced(worker: &Worker) -> bool {
    worker
        .snapshot
        .sources
        .iter()
        .any(|status| status.key == github() && matches!(status.state, SyncState::SignIn))
}

#[test]
fn linear_waits_out_its_own_interval_while_the_others_sync() {
    let dir = tempfile::tempdir().unwrap();
    let mut worker = worker(dir.path());
    let info = RepoInfo {
        repository: Repository::Local(dir.path().to_owned()),
        main_root: dir.path().to_string_lossy().into_owned(),
        beads: false,
        remote: Some(Remote {
            name: "origin".into(),
            url: "https://github.com/acme/app.git".into(),
            source: github(),
        }),
    };
    worker.snapshot.sources = statuses(&info, worker.linear_source());
    worker.snapshot.repo = Some(info);
    worker.linear_fetch = counted;
    let pages = || PAGES.load(Ordering::SeqCst);
    assert_eq!(worker.request.timing.linear, Duration::from_secs(300));

    worker.sync(None, Linear::Due);
    let first = pages();
    assert!(first > 0);
    assert!(github_synced(&worker));

    // A minute's sync later: GitHub syncs, Linear waits.
    mark(&mut worker);
    worker.sync(None, Linear::Due);
    assert!(github_synced(&worker));
    assert_eq!(pages(), first);

    // Refresh reads both.
    mark(&mut worker);
    worker.sync(None, Linear::Now);
    assert!(github_synced(&worker));
    assert_eq!(pages(), 2 * first);

    // A move, a new issue, a new key, or the section shown again reads
    // Linear alone.
    mark(&mut worker);
    worker.sync_linear_now();
    assert!(!github_synced(&worker));
    assert_eq!(pages(), 3 * first);

    // Once the interval has passed, the periodic sync reads Linear again.
    worker.request.timing.linear = Duration::ZERO;
    worker.sync(None, Linear::Due);
    assert_eq!(pages(), 4 * first);
}
