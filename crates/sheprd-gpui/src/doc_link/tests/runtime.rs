//! App lifetime and scheduling, with host calls captured instead of executed.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::doc_view::Docs;
use gpui::TestAppContext;
use sheprd_doc::{Author, Edit};
use std::{cell::Cell, collections::VecDeque, sync::Mutex, time::Duration};

#[path = "runtime_notifications.rs"]
mod notifications;
#[path = "runtime_recovery.rs"]
mod recovery;
#[path = "runtime_scheduling.rs"]
mod scheduling;

const QUIET: Duration = Duration::from_secs(25);
const NAME: &str = "doc-todo-123456";

struct Fixture {
    _dir: tempfile::TempDir,
    buffer: Entity<DocBuffer>,
    clock: gpui::BackgroundExecutor,
    launches: Arc<Mutex<Vec<String>>>,
    links: Entity<Links>,
    path: PathBuf,
    prompts: Arc<Mutex<Vec<String>>>,
    replies: Arc<Mutex<VecDeque<Result<Delivery>>>>,
}

fn agent() -> Launched {
    Launched {
        agent_name: NAME.into(),
        pane_id: "p0".into(),
        tab_id: "t0".into(),
    }
}

fn snapshot(present: bool, revision: u64) -> Arc<ClientShellSnapshot> {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
    snapshot.revision = revision;
    snapshot.agents.truncate(1);
    if present {
        snapshot.agents[0].name = Some(NAME.into());
    } else {
        snapshot.agents.clear();
    }
    Arc::new(snapshot)
}

fn fixture(cx: &mut TestAppContext) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("todo.md");
    std::fs::write(&path, "- [ ] first\n").unwrap();
    let buffer = cx.update(|cx| Docs::open(&path, cx));
    let clock = cx.background_executor.clone();
    let launches = Arc::new(Mutex::new(Vec::new()));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let replies = Arc::new(Mutex::new(VecDeque::new()));
    let links = cx.new(|cx| {
        let mut links = Links::new(None, cx);
        let launch_log = launches.clone();
        let prompt_log = prompts.clone();
        let outcomes = replies.clone();
        links.jobs = Jobs {
            launch: Arc::new(move |_, launch, _, _| {
                launch_log.lock().unwrap().push(launch.prompt.to_owned());
                LaunchResult {
                    result: Ok(agent()),
                    started: Some(agent()),
                }
            }),
            prompt: Arc::new(move |_, _, text, _| {
                prompt_log.lock().unwrap().push(text.to_owned());
                outcomes
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(Delivery::Sent))
            }),
        };
        links
    });
    cx.run_until_parked();
    Fixture {
        _dir: dir,
        buffer,
        clock,
        launches,
        links,
        path,
        prompts,
        replies,
    }
}

impl Fixture {
    fn enable(&self, present: bool, cx: &mut TestAppContext) {
        self.links.update(cx, |links, cx| {
            links
                .enable(
                    Enable {
                        buffer: self.buffer.clone(),
                        reveal: None,
                        scope: Scope::endpoint("fixture"),
                        snapshot: snapshot(present, 1),
                        target: ConnectTarget::Local,
                        workspace_id: "w0".into(),
                    },
                    cx,
                )
                .unwrap()
        });
        cx.run_until_parked();
    }

    fn user(&self, text: &str, cx: &mut TestAppContext) {
        self.buffer.update(cx, |buffer, cx| {
            let at = buffer.document().unwrap().text().len();
            buffer.edit(&[Edit::insert(at, text)], cx).unwrap();
        });
        cx.run_until_parked();
    }

    fn own(&self, cx: &mut TestAppContext) {
        self.buffer.update(cx, |buffer, cx| {
            let version = buffer.document().unwrap().version();
            buffer
                .agent_edit(
                    &Author {
                        pane: "p0".into(),
                        label: "conductor".into(),
                    },
                    sheprd_doc::Scope {
                        version,
                        section: None,
                    },
                    |text| Ok(vec![Edit::insert(text.len(), "status\n")]),
                    cx,
                )
                .unwrap();
        });
        cx.run_until_parked();
    }

    fn advance(&self, elapsed: Duration, cx: &mut TestAppContext) {
        self.clock.advance_clock(elapsed);
        cx.run_until_parked();
    }

    fn status(&self, cx: &mut TestAppContext) -> LinkStatus {
        cx.update(|cx| self.links.read(cx).status(&self.path).unwrap())
    }

    fn changes(&self) -> Vec<String> {
        self.prompts
            .lock()
            .unwrap()
            .iter()
            .filter(|text| text.contains("Mode: change\n"))
            .cloned()
            .collect()
    }

    fn observe(&self, present: bool, revision: u64, cx: &mut TestAppContext) {
        self.links.update(cx, |links, cx| {
            links.observe_endpoint(
                &Scope::endpoint("fixture"),
                Some(self.clock.now()),
                Some(&snapshot(present, revision)),
                cx,
            )
        });
        cx.run_until_parked();
    }

    fn record(&self, enabled: bool) -> Record {
        Record {
            agent_name: NAME.into(),
            enabled,
            pane_id: "p0".into(),
            path: self.path.clone(),
            scope: Scope::endpoint("fixture"),
            session: None,
            tab_id: "t0".into(),
            workspace_id: "w0".into(),
        }
    }
}

#[gpui::test]
fn a_burst_sends_one_change_after_its_last_edit(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.user("one\n", cx);
    f.advance(Duration::from_secs(10), cx);
    f.user("two\n", cx);
    f.advance(QUIET - Duration::from_millis(1), cx);
    assert!(f.changes().is_empty());
    f.advance(Duration::from_millis(1), cx);
    assert_eq!(f.changes().len(), 1);
    assert!(f.changes()[0].contains("two"));
    assert!(f.status(cx).pending.is_none());
}

#[gpui::test]
fn own_edits_are_silent_and_mixed_edits_keep_the_user_deadline(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.own(cx);
    f.advance(QUIET, cx);
    assert!(f.changes().is_empty());
    f.user("user\n", cx);
    f.advance(Duration::from_secs(10), cx);
    f.own(cx);
    f.advance(Duration::from_secs(15), cx);
    assert_eq!(f.changes().len(), 1);
    assert!(f.changes()[0].contains("user"));
    assert!(f.changes()[0].contains("status"));
}

#[gpui::test]
fn not_ready_pauses_and_retries_once_per_interval(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.replies
        .lock()
        .unwrap()
        .extend([Ok(Delivery::NotReady), Ok(Delivery::Sent)]);
    f.user("user\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(f.status(cx).mode, Mode::Paused);
    assert_eq!(f.changes().len(), 1);
    f.advance(QUIET - Duration::from_millis(1), cx);
    assert_eq!(f.changes().len(), 1);
    f.advance(Duration::from_millis(1), cx);
    assert_eq!(f.changes().len(), 2);
    assert_eq!(f.status(cx).mode, Mode::On);
}

#[gpui::test]
fn gone_turns_off_without_an_off_prompt(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.replies.lock().unwrap().push_back(Ok(Delivery::Gone));
    f.user("user\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert!(
        !f.prompts
            .lock()
            .unwrap()
            .iter()
            .any(|text| text.contains("Mode: off\n"))
    );
    assert!(!cx.update(|cx| f.links.read(cx).entries[0].record.as_ref().unwrap().enabled));
}

#[gpui::test]
fn disable_drops_pending_and_sends_off_only_once(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.user("unsent\n", cx);
    f.links.update(cx, |links, cx| {
        links.disable(&f.path, cx);
        links.disable(&f.path, cx);
    });
    cx.run_until_parked();
    f.advance(QUIET * 2, cx);
    let prompts = f.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("Mode: off\n"));
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert_eq!(f.launches.lock().unwrap().len(), 1);
}

#[gpui::test]
fn only_a_live_snapshot_after_sighting_can_close_the_agent(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.observe(false, 2, cx);
    assert_eq!(f.status(cx).mode, Mode::On);
    f.observe(true, 3, cx);
    f.links.update(cx, |links, cx| {
        links.observe_endpoint(
            &Scope::endpoint("fixture"),
            None,
            Some(&snapshot(false, 4)),
            cx,
        )
    });
    assert_eq!(f.status(cx).mode, Mode::On);
    f.observe(false, 2, cx);
    assert_eq!(f.status(cx).mode, Mode::On);
    f.observe(false, 4, cx);
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert!(f.prompts.lock().unwrap().is_empty());
}

#[gpui::test]
fn config_reload_rearms_the_pending_deadline(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.user("user\n", cx);
    cx.update(|cx| {
        let mut appearance = crate::app::InitialAppearance::default();
        appearance.config.doc_agent.debounce_seconds = 5;
        cx.set_global(appearance);
    });
    cx.run_until_parked();
    f.advance(Duration::from_secs(5), cx);
    assert_eq!(f.changes().len(), 1);
}

#[gpui::test]
fn undoing_all_pending_text_silently_acknowledges_the_empty_diff(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let initial = f.status(cx).acked;
    f.user("temporary\n", cx);
    let undone = f
        .buffer
        .update(cx, |buffer, cx| buffer.history(false, cx).unwrap());
    assert!(undone.is_some());
    cx.run_until_parked();
    f.advance(QUIET, cx);
    assert!(f.changes().is_empty());
    assert!(f.status(cx).pending.is_none());
    assert_ne!(f.status(cx).acked, initial);
}

#[gpui::test]
fn another_scope_cannot_close_or_retarget_the_link(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.observe(true, 2, cx);
    f.links.update(cx, |links, cx| {
        links.observe_endpoint(
            &Scope::endpoint("other"),
            Some(f.clock.now()),
            Some(&snapshot(false, 100)),
            cx,
        )
    });
    assert_eq!(f.status(cx).mode, Mode::On);
    assert_eq!(f.status(cx).pane_id.as_deref(), Some("p0"));
}

#[test]
fn each_mode_has_its_header_label() {
    crate::i18n::show(&sheprd_i18n::Locale::FALLBACK);
    for (mode, expected) in [
        (Mode::Off, "Connect agent"),
        (Mode::Starting, "Connecting agent"),
        (Mode::On, "Agent connected"),
        (Mode::Paused, "Agent paused"),
        (Mode::Restoring, "Reconnecting agent"),
    ] {
        assert_eq!(crate::doc_link::presentation::label(mode), expected);
    }
}
