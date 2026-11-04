use super::*;

#[gpui::test]
fn prompt_attempts_use_the_executor_deadlines_during_clock_advancement(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let recorded = attempts.clone();
    let clock = f.clock.clone();
    f.links.update(cx, |links, _| {
        links.jobs.prompt = Arc::new(move |_, _, _, _| {
            let mut attempts = recorded.lock().unwrap();
            attempts.push(clock.now());
            Ok(if attempts.len() == 1 {
                Delivery::NotReady
            } else {
                Delivery::Sent
            })
        });
    });
    let started = f.clock.now();
    f.user("work\n", cx);
    f.advance(QUIET, cx);
    assert_eq!(*attempts.lock().unwrap(), vec![started + QUIET]);
    f.advance(QUIET - Duration::from_millis(1), cx);
    assert_eq!(attempts.lock().unwrap().len(), 1);
    f.advance(Duration::from_millis(1), cx);
    assert_eq!(
        *attempts.lock().unwrap(),
        vec![started + QUIET, started + QUIET * 2]
    );
    assert_eq!(f.status(cx).mode, Mode::On);
}

#[gpui::test]
fn an_early_timer_wake_rearms_the_remaining_delay(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.user("work\n", cx);
    let clock = f.clock.clone();
    let lag = Duration::from_millis(1);
    f.links.update(cx, |links, _| {
        links.now = Rc::new(move || clock.now() - lag);
    });

    f.advance(QUIET, cx);
    assert!(f.changes().is_empty());
    f.advance(lag, cx);
    assert_eq!(f.changes().len(), 1);
    assert!(f.status(cx).pending.is_none());
}

#[gpui::test]
fn repeated_prompt_failures_back_off_to_five_minutes_and_sent_resets(cx: &mut TestAppContext) {
    for not_ready in [true, false] {
        let f = fixture(cx);
        f.enable(false, cx);
        for _ in 0..6 {
            f.replies.lock().unwrap().push_back(if not_ready {
                Ok(Delivery::NotReady)
            } else {
                Err(Error::Script {
                    step: crate::doc_link::JobStep::Prompt,
                    source: sheprd_client::Error::ScriptTimeout,
                })
            });
        }
        f.user("work\n", cx);
        f.advance(QUIET, cx);
        for (index, seconds) in [25, 50, 100, 200, 300, 300].into_iter().enumerate() {
            let delay = Duration::from_secs(seconds);
            assert_eq!(f.status(cx).mode, Mode::Paused);
            f.links.update(cx, |links, _| {
                let link = &links.entries[0];
                assert_eq!(link.retry_at, Some(f.clock.now() + delay));
                assert_eq!(link.push.as_ref().unwrap().deadline(), link.retry_at);
            });
            f.advance(delay - Duration::from_millis(1), cx);
            assert_eq!(f.changes().len(), index + 1);
            f.advance(Duration::from_millis(1), cx);
            assert_eq!(f.changes().len(), index + 2);
        }
        assert_eq!(f.status(cx).mode, Mode::On);
        f.replies.lock().unwrap().push_back(Ok(Delivery::NotReady));
        f.user("next work\n", cx);
        f.advance(QUIET, cx);
        f.links.update(cx, |links, _| {
            assert_eq!(links.entries[0].failures, 1);
            assert_eq!(links.entries[0].retry_at, Some(f.clock.now() + QUIET));
        });
        f.advance(QUIET, cx);
        assert_eq!(f.status(cx).mode, Mode::On);
    }
}

#[gpui::test]
fn start_and_resync_use_the_same_backoff(cx: &mut TestAppContext) {
    for kind in [ControlKind::Start, ControlKind::Resync] {
        let f = fixture(cx);
        f.replies
            .lock()
            .unwrap()
            .extend([Ok(Delivery::NotReady), Ok(Delivery::NotReady)]);
        let restoring = kind == ControlKind::Resync;
        f.links
            .update(cx, |links, cx| links.restore(vec![f.record(restoring)], cx));
        if restoring {
            f.observe(true, 1, cx);
        } else {
            f.enable(true, cx);
        }
        f.advance(QUIET, cx);
        f.links.update(cx, |links, _| {
            let link = &links.entries[0];
            assert_eq!(link.failures, 2);
            assert_eq!(link.retry_at, Some(f.clock.now() + QUIET * 2));
            assert_eq!(Some(link.control.as_ref().unwrap().retry_at), link.retry_at);
        });
        f.advance(QUIET, cx);
        assert_eq!(f.prompts.lock().unwrap().len(), 2);
        f.advance(QUIET, cx);
        assert_eq!(f.prompts.lock().unwrap().len(), 3);
        assert_eq!(f.status(cx).mode, Mode::On);
        f.links
            .update(cx, |links, _| assert_eq!(links.entries[0].failures, 0));
    }
}

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[gpui::test]
fn the_same_deadline_retains_its_timer_and_a_new_deadline_replaces_it(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.user("work\n", cx);
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(dropped.clone());
    let task = cx.background_executor.spawn(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    });
    let deadline = f.links.update(cx, |links, _| {
        let timer = links.entries[0].timer.as_mut().unwrap();
        timer._task = task;
        timer.deadline
    });
    cx.run_until_parked();

    f.own(cx);
    f.links.update(cx, |links, cx| {
        links.pump(cx);
        assert_eq!(links.entries[0].timer.as_ref().unwrap().deadline, deadline);
    });
    cx.run_until_parked();
    assert!(!dropped.load(Ordering::Acquire));

    f.advance(Duration::from_secs(1), cx);
    f.user("more work\n", cx);
    assert!(dropped.load(Ordering::Acquire));
    f.links.update(cx, |links, _| {
        assert_eq!(
            links.entries[0].timer.as_ref().unwrap().deadline,
            f.clock.now() + QUIET
        );
    });
}
