use super::*;

fn observe_status(f: &Fixture, status: AgentStatus, revision: u64, cx: &mut TestAppContext) {
    let mut current = (*snapshot(true, revision)).clone();
    current.agents[0].agent_status = status;
    f.links.update(cx, |links, cx| {
        links.observe_endpoint(
            &Scope::endpoint("fixture"),
            Some(f.clock.now()),
            Some(&Arc::new(current)),
            cx,
        )
    });
    cx.run_until_parked();
}

#[gpui::test]
fn restart_resyncs_a_present_agent_once_without_launching(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.links
        .update(cx, |links, cx| links.restore(vec![f.record(true)], cx));
    assert_eq!(f.status(cx).mode, Mode::Restoring);
    f.observe(true, 1, cx);
    f.observe(true, 1, cx);
    assert_eq!(f.status(cx).mode, Mode::On);
    assert!(f.launches.lock().unwrap().is_empty());
    let prompts = f.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(prompts[0].contains("Mode: resync\n"));
}

#[gpui::test]
fn restart_without_the_agent_saves_off_and_sends_nothing(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.links
        .update(cx, |links, cx| links.restore(vec![f.record(true)], cx));
    f.observe(false, 1, cx);
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert!(f.prompts.lock().unwrap().is_empty());
    assert!(f.launches.lock().unwrap().is_empty());
    assert!(!cx.update(|cx| f.links.read(cx).entries[0].record.as_ref().unwrap().enabled));
}

#[gpui::test]
fn reenable_reuses_the_live_agent_and_sends_start_again(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.links.update(cx, |links, cx| links.disable(&f.path, cx));
    cx.run_until_parked();
    f.enable(true, cx);
    assert_eq!(f.launches.lock().unwrap().len(), 1);
    assert_eq!(f.status(cx).mode, Mode::On);
    let prompts = f.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].contains("Mode: off\n"));
    assert!(prompts[1].contains("Mode: on\n"));
}

#[gpui::test]
fn a_failed_initial_prompt_keeps_the_started_agent_and_retries_start_first(
    cx: &mut TestAppContext,
) {
    let f = fixture(cx);
    let launches = f.launches.clone();
    f.links.update(cx, |links, _| {
        links.jobs.launch = Arc::new(move |_, launch, _, _| {
            launches.lock().unwrap().push(launch.prompt.to_owned());
            LaunchResult {
                result: Err(Error::NotReady {
                    source: sheprd_client::Error::ScriptCancelled,
                }),
                started: Some(agent()),
            }
        });
    });
    f.enable(false, cx);
    assert_eq!(f.status(cx).mode, Mode::Paused);
    assert!(cx.update(|cx| f.links.read(cx).entries[0].record.as_ref().unwrap().enabled));
    f.user("new work\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(f.launches.lock().unwrap().len(), 1);
    assert_eq!(f.status(cx).mode, Mode::On);
    let prompts = f.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].contains("Mode: on\n"));
    assert!(prompts[1].contains("Mode: change\n"));
    assert!(prompts[1].contains("new work"));
}

#[gpui::test]
fn a_failure_before_agent_started_leaves_the_link_off(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.links.update(cx, |links, _| {
        links.jobs.launch = Arc::new(|_, _, _, _| LaunchResult {
            result: Err(Error::Cancelled),
            started: None,
        });
    });
    f.enable(false, cx);
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert!(cx.update(|cx| matches!(f.links.read(cx).error(&f.path), Some(Error::Cancelled))));
    f.advance(QUIET * 2, cx);
    assert!(f.prompts.lock().unwrap().is_empty());
}

#[gpui::test]
fn failed_resync_retries_before_any_change_and_retains_edits(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.replies.lock().unwrap().push_back(Ok(Delivery::NotReady));
    f.links
        .update(cx, |links, cx| links.restore(vec![f.record(true)], cx));
    f.observe(true, 1, cx);
    f.user("work during resync\n", cx);
    assert_eq!(f.status(cx).mode, Mode::Paused);
    f.advance(QUIET, cx);
    let prompts = f.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 3);
    assert!(prompts[0].contains("Mode: resync\n"));
    assert!(prompts[1].contains("Mode: resync\n"));
    assert!(prompts[2].contains("Mode: change\n"));
}

#[gpui::test]
fn a_ready_transition_retries_changes_before_the_retry_interval(cx: &mut TestAppContext) {
    for ready in [AgentStatus::Idle, AgentStatus::Done, AgentStatus::Working] {
        let f = fixture(cx);
        f.enable(false, cx);
        observe_status(&f, AgentStatus::Blocked, 2, cx);
        f.replies.lock().unwrap().push_back(Ok(Delivery::NotReady));
        f.user("work\n", cx);
        f.advance(QUIET, cx);
        assert_eq!(f.status(cx).mode, Mode::Paused);
        assert_eq!(f.changes().len(), 1);

        f.advance(Duration::from_secs(1), cx);
        assert_eq!(f.changes().len(), 1);
        observe_status(&f, ready, 3, cx);
        assert_eq!(f.changes().len(), 2);
        assert_eq!(f.status(cx).mode, Mode::On);
        f.advance(QUIET, cx);
        assert_eq!(f.changes().len(), 2);
    }
}

#[gpui::test]
fn a_ready_transition_releases_pending_start_and_resync(cx: &mut TestAppContext) {
    for kind in [ControlKind::Start, ControlKind::Resync] {
        let f = fixture(cx);
        f.replies.lock().unwrap().push_back(Ok(Delivery::NotReady));
        let restoring = kind == ControlKind::Resync;
        f.links
            .update(cx, |links, cx| links.restore(vec![f.record(restoring)], cx));
        if restoring {
            f.observe(true, 1, cx);
        } else {
            f.enable(true, cx);
        }
        assert_eq!(f.status(cx).mode, Mode::Paused);
        assert_eq!(f.prompts.lock().unwrap().len(), 1);
        observe_status(&f, AgentStatus::Blocked, 2, cx);

        f.advance(Duration::from_secs(1), cx);
        assert_eq!(f.prompts.lock().unwrap().len(), 1);
        observe_status(&f, AgentStatus::Idle, 3, cx);
        assert_eq!(f.status(cx).mode, Mode::On);
        assert!(f.launches.lock().unwrap().is_empty());
        let prompts = f.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0], prompts[1]);
    }
}

#[gpui::test]
fn a_transport_error_keeps_the_unacknowledged_range(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let acked = f.status(cx).acked;
    f.replies.lock().unwrap().push_back(Err(Error::Script {
        step: crate::doc_link::JobStep::Prompt,
        source: sheprd_client::Error::ScriptCancelled,
    }));
    f.user("work\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(f.status(cx).acked, acked);
    assert_eq!(f.status(cx).mode, Mode::Paused);
    f.advance(QUIET, cx);
    assert_eq!(f.changes().len(), 2);
    assert_ne!(f.status(cx).acked, acked);
}

#[gpui::test]
fn shutdown_cancels_the_job_and_flushes_off_without_sending_an_off_prompt(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let state = f._dir.path().join("links.json");
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let task = f.links.update(cx, |links, cx| {
        links.persistence = Persistence::new(Some(state.clone()));
        links.entries[0].job = Some(Flight {
            _task: Task::ready(()),
            cancelled: flag,
        });
        links.disable(&f.path, cx);
        links.shutdown(cx)
    });
    assert!(cancelled.load(Ordering::Acquire));
    let task = cx.background_executor.spawn(async move {
        if let Some(task) = task {
            task.await;
        }
    });
    task.detach();
    cx.run_until_parked();
    assert!(f.prompts.lock().unwrap().is_empty());
    let records = crate::doc_link::load(&state).unwrap();
    assert!(!records[0].enabled);
}

#[gpui::test]
fn off_waits_for_cancelled_work_and_does_not_drop_its_slot(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    f.links.update(cx, |links, cx| {
        links.entries[0].job = Some(Flight {
            _task: Task::ready(()),
            cancelled: flag,
        });
        links.disable(&f.path, cx);
        assert!(links.entries[0].job.is_some());
    });
    cx.run_until_parked();
    assert!(cancelled.load(Ordering::Acquire));
    assert!(f.prompts.lock().unwrap().is_empty());
    f.links.update(cx, |links, cx| {
        links.entries[0].job = None;
        links.pump(cx);
    });
    cx.run_until_parked();
    assert_eq!(f.prompts.lock().unwrap().len(), 1);
}

#[gpui::test]
fn shutdown_keeps_an_enabled_record_for_resync(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let state = f._dir.path().join("enabled.json");
    let task = f.links.update(cx, |links, cx| {
        links.persistence = Persistence::new(Some(state.clone()));
        links.shutdown(cx)
    });
    cx.background_executor
        .spawn(async move {
            if let Some(task) = task {
                task.await;
            }
        })
        .detach();
    cx.run_until_parked();
    assert!(crate::doc_link::load(&state).unwrap()[0].enabled);
    assert!(f.prompts.lock().unwrap().is_empty());
}

#[gpui::test]
fn repeated_enable_shares_one_subscription_and_agent(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.enable(false, cx);
    f.user("one\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(f.launches.lock().unwrap().len(), 1);
    assert_eq!(f.changes().len(), 1);
}

#[gpui::test]
fn the_enabled_limit_counts_restoring_links(cx: &mut TestAppContext) {
    let f = fixture(cx);
    let records: Vec<_> = (0..crate::doc_link::MAX_LINKS)
        .map(|n| {
            let mut record = f.record(true);
            record.path = f._dir.path().join(format!("saved-{n}.md"));
            std::fs::write(&record.path, "").unwrap();
            record
        })
        .collect();
    let result = f.links.update(cx, |links, cx| {
        links.restore(records, cx);
        links.enable(
            Enable {
                buffer: f.buffer.clone(),
                reveal: None,
                scope: Scope::endpoint("fixture"),
                snapshot: snapshot(false, 1),
                target: ConnectTarget::Local,
                workspace_id: "w0".into(),
            },
            cx,
        )
    });
    assert!(matches!(result, Err(Error::TooMany { .. })));
}

#[test]
fn target_support_is_explicit_and_not_a_posix_gate() {
    use crate::doc_link::window::host;
    assert!(host(&ConnectTarget::Local).is_ok());
    assert!(
        host(&ConnectTarget::Session {
            name: "named".into(),
            development: false
        })
        .is_ok()
    );
    for target in [
        ConnectTarget::Session {
            name: "named".into(),
            development: true,
        },
        ConnectTarget::Socket(PathBuf::from("/tmp/socket")),
        ConnectTarget::Ssh {
            target: "example".into(),
            session: "named".into(),
        },
    ] {
        assert!(matches!(host(&target), Err(Error::Unsupported { .. })));
    }
}
