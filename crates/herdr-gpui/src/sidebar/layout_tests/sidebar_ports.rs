//! `[sidebar] ports` and Toggle Sidebar Ports take the ports line from under
//! sidebar rows; the status bar keeps the focused workspace's ports, and
//! `show_listening_ports` still hides both.
use super::*;
use std::sync::Mutex;

/// The fixture with its first workspace focused and listening on :3000.
fn seeded(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    let mut view = fixture_window(window, cx);
    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let id = snapshot.workspaces[0].workspace_id.clone();
    snapshot.focused_workspace_id = Some(id.clone());
    let ports = format!("L 1 *:3000 node\nE 1 {id}\n");
    view.listening_ports.seed(
        crate::usage::Host::Local,
        crate::listening_ports::parse(&ports).unwrap(),
    );
    view
}

/// The probe name of the first workspace's sidebar ports line.
fn ports_line(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> &'static str {
    let id = view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_ref().unwrap();
        snapshot.workspaces[0].workspace_id.clone()
    });
    // Probe names are static; leaking one in a test is fine.
    format!("ports-local-{id}").leak()
}

#[gpui::test]
fn sidebar_ports_off_hides_the_row_line_but_not_the_status_bar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(seeded);
    cx.simulate_resize(size(px(800.), px(600.)));
    let line = ports_line(&view, cx);
    for shown in [true, false, true] {
        view.update(cx, |view, cx| {
            view.config.sidebar_style.ports = shown;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(
            cx.debug_bounds(line).is_some(),
            shown,
            "sidebar ports {shown}"
        );
        assert!(
            cx.debug_bounds("listening-ports").is_some(),
            "sidebar ports {shown}"
        );
    }
    view.update(cx, |view, cx| {
        view.config.show_listening_ports = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds(line).is_none());
    assert!(cx.debug_bounds("listening-ports").is_none());
}

#[gpui::test]
fn toggle_sidebar_ports_flips_the_setting_and_saves_each_value(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(seeded);
    cx.simulate_resize(size(px(800.), px(600.)));
    let line = ports_line(&view, cx);
    let saved = Arc::new(Mutex::new(Vec::new()));
    view.update(cx, |view, cx| {
        // A load that never finishes, so the reload after each save does not
        // read the real config; the save itself still runs.
        view.config_load = Some(cx.spawn(async |_, _| std::future::pending::<()>().await));
    });
    for shown in [false, true] {
        let saves = saved.clone();
        view.update(cx, |view, cx| {
            view.toggle_sidebar_ports_with(
                move |value| {
                    saves.lock().unwrap().push(value);
                    Ok(())
                },
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.config.sidebar_style.ports),
            shown
        );
        assert!(view.read_with(cx, |view, _| view.config.show_listening_ports));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(
            cx.debug_bounds(line).is_some(),
            shown,
            "sidebar ports {shown}"
        );
        assert!(
            cx.debug_bounds("listening-ports").is_some(),
            "sidebar ports {shown}"
        );
    }
    assert_eq!(*saved.lock().unwrap(), [false, true]);
}
