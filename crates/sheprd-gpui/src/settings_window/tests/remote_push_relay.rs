//! Settings > Remote control > the push relay: turning it on asks first
//! with what the push relay sees, turning it off does not, and the page
//! shows how the daemon's pushes leave it and how the last ones went.
use super::{
    remote_control::{Edits, click, draw, page},
    *,
};
use crate::{
    config::{preferences::Preference, remote::Edit},
    remote::Supervisor,
    settings_window::pages::remote_control::{delivery_line, last_alert, last_live, last_worked},
};
use sheprd_remote_control::control::push::{Last, Mode, Outcome, PushStatus};

/// `text` without Fluent's isolation marks around its arguments.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn last(at_ms: u64, outcome: Outcome) -> Option<Last> {
    Some(Last { at_ms, outcome })
}

/// The page with the push relay off, which is on by default: this
/// computer's user turned it off before.
fn turned_off(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext, Edits) {
    let (view, cx, edits) = page(cx);
    view.update(cx, |view, cx| {
        view.config.remote.push_relay = false;
        cx.notify();
    });
    draw(cx);
    (view, cx, edits)
}

#[gpui::test]
fn turning_the_push_relay_on_asks_first(cx: &mut TestAppContext) {
    let (view, cx, edits) = turned_off(cx);
    click(cx, "remote-push-relay-toggle");
    assert!(
        edits.lock().unwrap().is_empty(),
        "nothing saved before asking"
    );
    view.read_with(cx, |view, _| assert!(view.remote.push_relay.is_open()));
    // The explanation names what the push relay sees and never sees.
    assert!(cx.debug_bounds("remote-push-relay-ask").is_some());
    let english = crate::i18n::english("settings-push-relay-ask-never", &[]);
    assert!(english.contains("prompts, chats, code"));

    click(cx, "remote-push-relay-cancel");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert!(edits.lock().unwrap().is_empty());

    click(cx, "remote-push-relay-toggle");
    click(cx, "remote-push-relay-confirm");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert_eq!(
        *edits.lock().unwrap(),
        [Preference::Remote(Edit::PushRelay(true))]
    );
}

#[gpui::test]
fn turning_the_push_relay_off_needs_no_asking(cx: &mut TestAppContext) {
    let (view, cx, edits) = page(cx);
    view.read_with(cx, |view, _| {
        assert!(view.config.remote.push_relay, "on by default")
    });
    click(cx, "remote-push-relay-toggle");
    view.read_with(cx, |view, _| assert!(!view.remote.push_relay.is_open()));
    assert_eq!(
        *edits.lock().unwrap(),
        [Preference::Remote(Edit::PushRelay(false))]
    );
}

#[gpui::test]
fn the_page_shows_how_pushes_leave_and_how_the_last_went(cx: &mut TestAppContext) {
    let (view, cx, _) = page(cx);
    assert!(cx.debug_bounds("remote-push-delivery-last").is_none());
    let now = crate::remote::now_ms();
    cx.update(|_, cx| {
        Supervisor::set_push_status(
            cx,
            Some(PushStatus {
                mode: Mode::PushRelay,
                alert: last(now, Outcome::Delivered),
                live_activity: None,
            }),
        );
    });
    view.update(cx, |_, cx| cx.notify());
    draw(cx);
    assert!(cx.debug_bounds("remote-push-delivery-last").is_some());
    assert!(cx.debug_bounds("remote-push-delivery-dot").is_some());
}

#[test]
fn delivery_lines_name_the_mode_and_each_outcome() {
    assert_eq!(delivery_line(None), "Shown while remote control runs.");
    assert_eq!(
        delivery_line(Some(&PushStatus::default())),
        "Off: the push relay is off."
    );
    let relayed = PushStatus {
        mode: Mode::PushRelay,
        ..PushStatus::default()
    };
    assert_eq!(delivery_line(Some(&relayed)), "Through the push relay.");
    // An older daemon that sent with its own APNs key says `own_key`.
    let older: PushStatus = serde_json::from_str(r#"{"mode":"own_key"}"#).unwrap();
    assert!(delivery_line(Some(&older)).contains("does not know"));

    assert_eq!(last_alert(None, 0), "No alert since the daemon started.");
    assert_eq!(
        plain(last_alert(last(0, Outcome::Delivered).as_ref(), 0)),
        "Last alert delivered just now."
    );
    let refused = Outcome::Refused {
        status: 403,
        reason: None,
    };
    assert_eq!(
        plain(last_live(last(0, refused).as_ref(), 120_000)),
        "Last Live Activity push 2 min ago: refused (403)."
    );
}

#[test]
fn the_dot_follows_the_most_recent_push() {
    assert_eq!(last_worked(&PushStatus::default()), None);
    let status = PushStatus {
        mode: Mode::PushRelay,
        alert: last(5, Outcome::Delivered),
        live_activity: last(9, Outcome::Unreachable),
    };
    assert_eq!(last_worked(&status), Some(false));
}

#[gpui::test]
fn turn_on_waits_while_another_setting_saves(cx: &mut TestAppContext) {
    let (view, cx, edits) = turned_off(cx);
    click(cx, "remote-push-relay-toggle");
    view.update(cx, |view, cx| {
        view.saving = true;
        cx.notify();
    });
    draw(cx);
    click(cx, "remote-push-relay-confirm");
    view.update(cx, |view, cx| {
        // The edit would be refused, so the explanation stays to ask again.
        assert!(view.remote.push_relay.is_open());
        view.confirm_push_relay(cx);
        assert!(view.remote.push_relay.is_open());
    });
    assert!(edits.lock().unwrap().is_empty());
}
