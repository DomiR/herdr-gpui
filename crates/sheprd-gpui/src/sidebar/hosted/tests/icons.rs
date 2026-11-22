//! The activity bar's icons for a host's layout: each panel's is Sheprd's
//! own, under the `app-` name a Moonframe mod draws it by.
use super::*;
use gpui::AssetSource as _;

#[gpui::test]
fn every_panel_s_icon_is_the_one_sheprd_s_bar_draws(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let state = view.update(cx, |view, cx| view.hosted_state(cx));
    let panels = state.main["panels"].as_array().unwrap();
    assert!(!panels.is_empty());
    for panel in panels {
        let name = panel["icon"].as_str().unwrap();
        assert!(name.starts_with("app-"), "{panel}");
        let own = format!("icons/{}.svg", name.trim_start_matches("app-"));
        let hosted = crate::icons::hosted::HostIcons
            .load(&format!("icons/{name}.svg"))
            .unwrap();
        assert!(hosted.is_some(), "{name} loads nothing");
        assert_eq!(hosted, crate::icons::Icons.load(&own).unwrap(), "{name}");
    }
    // The panel the round fallback once stood in for.
    assert!(panels.iter().any(|panel| panel["icon"] == "app-sessions"));
}

#[test]
fn the_luau_bar_s_settings_icon_is_sheprd_s() {
    let svg = crate::icons::hosted::HostIcons
        .load("icons/app-settings.svg")
        .unwrap();
    assert_eq!(svg, crate::icons::Icons.load("icons/settings.svg").unwrap());
    assert!(svg.is_some());
}
