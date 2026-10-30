use super::*;
use herdr_mods::{Container, Gap, Manifest, Needs, Node, Place, Size, Text, Tone, Weight};

fn text(text: &str) -> Node {
    Node::Text(Text {
        text: text.to_owned(),
        tone: Tone::Normal,
        size: Size::Normal,
        weight: Weight::Normal,
        truncate: false,
    })
}

fn manifest(name: &str, place: Place) -> Manifest {
    Manifest {
        name: name.to_owned(),
        places: vec![place],
        commands: Vec::new(),
        activity: None,
        needs: Needs::default(),
    }
}

/// Many tall sidebar mods share one area of at most 40% of the sidebar, so
/// the workspace and agent lists keep the rest.
#[gpui::test]
fn sidebar_mods_never_take_the_lists_height(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        for index in 0..5 {
            let lines = (0..12).map(|line| text(&format!("mod {index} line {line}")));
            let tree = Node::Column(Container {
                gap: Gap::Small,
                align: herdr_mods::Align::Stretch,
                children: lines.collect(),
            });
            view.mods
                .insert_drawn(manifest(&format!("tall{index}"), Place::Sidebar), tree);
        }
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let mods = cx.debug_bounds("mod-sidebar").unwrap();
    assert!(
        mods.size.height <= sidebar.size.height * 0.4 + px(1.),
        "{mods:?} in {sidebar:?}"
    );
    let spaces = cx.debug_bounds("spaces-section").unwrap();
    assert!(spaces.size.height > px(80.), "{spaces:?}");
    // The footer under the mods still shows inside the sidebar.
    assert!(mods.bottom() <= sidebar.bottom());
}

/// Wide status mods share one area that gives way before the app's own
/// controls.
#[gpui::test]
fn status_mods_never_push_out_the_apps_controls(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        for index in 0..4 {
            let label = format!("a very wide status bar item number {index} with a long label");
            view.mods.insert_drawn(
                manifest(&format!("wide{index}"), Place::StatusBar),
                text(&label),
            );
        }
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let area = cx.debug_bounds("mod-status").unwrap();
    assert!(area.size.width <= px(360.), "{area:?}");
    let bar = cx.debug_bounds("connection-status").unwrap();
    let theme = cx.debug_bounds("status-theme").unwrap();
    assert!(theme.size.width > px(0.));
    assert!(theme.right() <= bar.right(), "{theme:?} past {bar:?}");
}
