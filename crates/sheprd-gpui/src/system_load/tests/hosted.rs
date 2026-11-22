//! The load a layout that draws its own status bar is sent.
use super::*;
use crate::{config::status_bar::Detail, sidebar::layout_tests::fixture_window};
use gpui::TestAppContext;

fn sample(cpu: f32) -> Sample {
    Sample {
        cpu: Some(cpu),
        memory: Some(Memory {
            used: 4 << 30,
            total: 16 << 30,
        }),
        ..Sample::default()
    }
}

#[gpui::test]
fn the_state_carries_the_sparkline_and_meter_the_bar_draws(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_system_load = true;
        view.system_load.record(Host::Local, sample(20.));
        view.system_load.record(Host::Local, sample(80.));
        view
    });
    view.update(cx, |view, _| {
        let load = view.hosted_load();
        let history = load["history"].as_array().unwrap();
        // Unsampled bars wait on the left; the newest is last.
        assert_eq!(history.len(), HISTORY);
        assert_eq!(history[0], false);
        let newest = &history[HISTORY - 1];
        assert!((newest["value"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        // Past the CPU warning, the bar and the share take its color.
        let warning = crate::sidebar::hosted::hex(view.theme.ink(view.theme.palette[3]));
        assert_eq!(newest["color"], warning.as_str());
        assert_eq!(load["cpu_color"], warning.as_str());
        assert!((load["meter"]["used"].as_f64().unwrap() - 0.25).abs() < 1e-6);

        view.config.status_bar.system_load = Detail::Compact;
        let compact = view.hosted_load();
        assert!(compact.get("history").is_none() && compact.get("meter").is_none());
        assert_eq!(compact["cpu"].as_f64(), Some(80.));

        view.config.show_system_load = false;
        assert!(view.hosted_load().is_null());
    });
}
