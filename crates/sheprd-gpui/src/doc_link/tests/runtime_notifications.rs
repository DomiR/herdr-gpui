use super::*;

#[gpui::test]
fn unchanged_agent_snapshots_do_not_notify_link_observers(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    f.observe(true, 2, cx);
    let before = f.status(cx);
    let notified = Rc::new(Cell::new(0));
    let _observer = cx.update(|cx| {
        let notified = notified.clone();
        cx.observe(&f.links, move |_, _| notified.set(notified.get() + 1))
    });

    f.observe(true, 3, cx);
    f.observe(true, 4, cx);
    assert_eq!(f.status(cx), before);
    assert_eq!(notified.get(), 0);
}

#[gpui::test]
fn edits_do_not_notify_link_observers_but_mode_changes_do(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let notified = Rc::new(Cell::new(0));
    let _observer = cx.update(|cx| {
        let notified = notified.clone();
        cx.observe(&f.links, move |_, _| notified.set(notified.get() + 1))
    });

    f.user("one\n", cx);
    f.user("two\n", cx);
    f.buffer.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(notified.get(), 0);
    assert_eq!(f.status(cx).mode, Mode::On);

    f.links.update(cx, |links, cx| links.disable(&f.path, cx));
    cx.run_until_parked();
    assert_eq!(f.status(cx).mode, Mode::Off);
    assert!(notified.get() > 0);
}

#[gpui::test]
fn an_empty_diff_does_not_notify_link_observers(cx: &mut TestAppContext) {
    let f = fixture(cx);
    f.enable(false, cx);
    let notified = Rc::new(Cell::new(0));
    let _observer = cx.update(|cx| {
        let notified = notified.clone();
        cx.observe(&f.links, move |_, _| notified.set(notified.get() + 1))
    });

    f.user("temporary\n", cx);
    let undone = f
        .buffer
        .update(cx, |buffer, cx| buffer.history(false, cx).unwrap());
    assert!(undone.is_some());
    cx.run_until_parked();
    f.advance(QUIET, cx);
    assert!(f.changes().is_empty());
    assert!(f.status(cx).pending.is_none());
    assert_eq!(notified.get(), 0);
}
