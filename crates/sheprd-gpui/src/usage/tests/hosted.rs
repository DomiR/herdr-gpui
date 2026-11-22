//! The usage a layout that draws its own status bar is sent.
use super::*;
use crate::config::status_bar::Detail;
use crate::sidebar::layout_tests::fixture_window;
use gpui::TestAppContext;

fn claude(used: f64) -> Reading {
    let provider = provider("claude");
    Reading {
        provider,
        report: Some(report(provider, used)),
        error: None,
        access: None,
    }
}

#[gpui::test]
fn the_state_carries_the_segments_the_bar_draws(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.usage.show = true;
        view.usage = super::super::Usage::showing(super::super::Host::Local, vec![claude(70.)]);
        view
    });
    view.update(cx, |view, _| {
        let usage = view.hosted_usage();
        let segment = &usage["segments"][0];
        assert_eq!(segment["id"], "claude");
        assert_eq!(segment["locked"], false);
        // The meter is a fraction, in the warning's color past 60%.
        let used = segment["meter"]["used"].as_f64().unwrap();
        assert!((used - 0.7).abs() < 1e-6, "{used}");
        let warning = crate::sidebar::hosted::hex(view.theme.ink(view.theme.palette[3]));
        assert_eq!(segment["meter"]["color"], warning.as_str());
        // Fluent isolates its arguments, as the bar's own label does.
        let label: String = segment["labels"][0]["text"]
            .as_str()
            .unwrap()
            .chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect();
        assert!(label.starts_with("70% used"), "{label}");

        view.config.status_bar.usage = Detail::Compact;
        let compact = &view.hosted_usage()["segments"][0];
        assert!(compact.get("meter").is_none());
        assert_eq!(compact["labels"][0]["text"], "70%");

        view.config.usage.show = false;
        assert!(view.hosted_usage().is_null());
    });
}
