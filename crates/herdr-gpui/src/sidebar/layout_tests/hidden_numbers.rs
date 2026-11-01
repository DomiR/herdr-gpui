//! `[sidebar] numbers = false` draws none of the sidebar's number
//! indicators and nothing in their place; the default draws them all.
use super::*;
use crate::{
    config::{ChangeCounts, LayoutMode},
    git::{Counted, Status},
    state::ConnectionStatus,
};

/// Text the rows draw for the fixture's numbers: the pull request, its
/// counts, the local counts, and the upstream drift.
const NUMBERS: [&str; 7] = ["#7", "+234", "-567", "+42", "-9", "\u{2193}18", "\u{2191}2"];

fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let mut snapshot = snapshot(6);
            for (index, counts) in [(0, (0, 18)), (4, (2, 3))] {
                snapshot.workspaces[index].git_ahead_behind = Some(counts);
            }
            view.live.snapshot = Some(Arc::new(snapshot));
            view.endpoints[0].live = view.live.clone();
            let input = |branch: &str| crate::pull_request::Input {
                checkout: None,
                repo_key: Some(REPO_KEY.into()),
                branch: branch.into(),
            };
            let now = std::time::Instant::now();
            let mut pr = crate::pull_request::fixture().unwrap();
            pr.number = 7;
            pr.additions = 234;
            pr.deletions = 567;
            view.seed_pr(input("worktree/sidebar-child"), pr, now);
            // The head has uncommitted lines, which local counts would show.
            let counted = Counted {
                lines: Status {
                    additions: 42,
                    deletions: 9,
                    untracked: 0,
                },
                dirty: true,
            };
            view.git.seed_counts(input("develop"), counted, now);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    (view, cx)
}

fn set(
    view: &Entity<HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
    change: impl FnOnce(&mut HerdrWindow),
) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            change(view);
            cx.notify();
        })
    });
}

/// Draws a frame and returns every text it labelled.
fn drawn(cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        window.refresh();
        full_draw(window, cx).clear(cx);
        cx.global::<TextProbes>().0.keys().cloned().collect()
    })
}

fn numbers_in(texts: &[String]) -> Vec<&str> {
    NUMBERS
        .into_iter()
        .filter(|number| texts.iter().any(|text| text == number))
        .collect()
}

#[gpui::test]
fn rows_draw_their_numbers_only_with_the_setting_on(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let texts = drawn(cx);
    for number in ["#7", "+234", "-567", "\u{2193}18"] {
        assert!(texts.contains(&number.to_owned()), "missing {number}");
    }
    assert!(cx.debug_bounds("pr-sidebar-child").is_some());
    let named = cx.debug_bounds("name-sidebar-child").unwrap();
    let fixture_mode = view.read_with(cx, |view, _| view.config.layout.mode);

    set(&view, cx, |view| view.config.sidebar_style.numbers = false);
    for mode in LayoutMode::ALL {
        set(&view, cx, |view| view.config.layout.mode = mode);
        let texts = drawn(cx);
        assert_eq!(numbers_in(&texts), Vec::<&str>::new(), "{mode:?}");
        // Nothing stands in for the badge, so its column is gone.
        assert!(cx.debug_bounds("pr-sidebar-child").is_none(), "{mode:?}");
        assert!(cx.debug_bounds("upstream-herdr").is_none(), "{mode:?}");
    }
    set(&view, cx, |view| view.config.layout.mode = fixture_mode);
    drawn(cx);
    // The name takes back the room the badge reserved.
    let bare = cx.debug_bounds("name-sidebar-child").unwrap();
    assert!(bare.size.width > named.size.width, "{bare:?} {named:?}");

    // Local counts go too; the head's uncommitted work keeps its pencil,
    // which was never a number.
    set(&view, cx, |view| {
        view.config.sidebar_style.change_counts = ChangeCounts::Local;
    });
    assert_eq!(numbers_in(&drawn(cx)), Vec::<&str>::new());
    assert!(cx.debug_bounds("counts-agent-launcher").is_none());
    assert!(cx.debug_bounds("dirty-agent-launcher").is_some());
    set(&view, cx, |view| view.config.sidebar_style.numbers = true);
    let texts = drawn(cx);
    for number in ["+42", "-9"] {
        assert!(texts.contains(&number.to_owned()), "missing {number}");
    }
}

#[gpui::test]
fn a_configured_git_status_token_draws_nothing_without_numbers(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    set(&view, cx, |view| {
        view.config.sidebar_layout = toml::from_str(
            r#"
            [spaces]
            rows = [["workspace", "git_status"]]
        "#,
        )
        .unwrap();
    });
    assert!(drawn(cx).contains(&"\u{2193}18".to_owned()));
    set(&view, cx, |view| view.config.sidebar_style.numbers = false);
    let texts = drawn(cx);
    assert!(
        texts
            .iter()
            .all(|text| !text.starts_with(['\u{2191}', '\u{2193}']))
    );
    assert!(texts.contains(&"herdr".to_owned()));
}

#[gpui::test]
fn the_rail_drops_workspace_numbers_and_their_column(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    set(&view, cx, |view| view.sidebar_visible = false);
    let digits = |texts: &[String]| {
        (1..=6)
            .filter(|number| texts.contains(&number.to_string()))
            .count()
    };
    assert_eq!(digits(&drawn(cx)), 6);
    let mark = "rail-mark-rail-workspace-local-w0";
    let numbered = cx.debug_bounds(mark).unwrap();

    set(&view, cx, |view| view.config.sidebar_style.numbers = false);
    assert_eq!(digits(&drawn(cx)), 0);
    // Agent rows still use the column for their icon, so it stays, no
    // wider than they need, and the dots keep one line.
    let empty = cx.debug_bounds(mark).unwrap();
    assert!(empty.size.width <= numbered.size.width);
    let dot = cx.debug_bounds("rail-dot-rail-workspace-local-w0").unwrap();
    let agent = cx.debug_bounds("rail-dot-rail-agent-local-p0").unwrap();
    assert_eq!(dot.left(), agent.left());

    // With no agent rows nothing needs the column, so there is none.
    set(&view, cx, |view| view.config.show_agents = false);
    drawn(cx);
    assert!(cx.debug_bounds(mark).is_none());
    let rail = cx.debug_bounds("sidebar-rail").unwrap();
    let dot = cx.debug_bounds("rail-dot-rail-workspace-local-w0").unwrap();
    let off_centre = (dot.center().x - rail.center().x).abs();
    assert!(off_centre < px(2.), "{dot:?} {rail:?}");
}

#[gpui::test]
fn host_tallies_and_footer_counts_follow_the_setting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    set(&view, cx, |view| {
        view.config.layout.mode = LayoutMode::Devices;
        view.config.sidebar_style.activity_bar = true;
        view.live.status = ConnectionStatus::Connected;
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Build box".into(),
            ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = Some(Arc::new(snapshot(1)));
        remote.live.status = ConnectionStatus::Connected;
        view.endpoints.push(remote);
    });
    drawn(cx);
    assert!(cx.debug_bounds("host-working-local").is_some());
    assert!(cx.debug_bounds("device-footer-hosts").is_some());
    let named = cx.debug_bounds("host-label-local").unwrap();

    set(&view, cx, |view| view.config.sidebar_style.numbers = false);
    drawn(cx);
    assert!(cx.debug_bounds("host-working-local").is_none());
    assert!(cx.debug_bounds("host-working-ssh:test").is_none());
    assert!(cx.debug_bounds("device-footer-hosts").is_none());
    // The host's name takes back the tally's room.
    let bare = cx.debug_bounds("host-label-local").unwrap();
    assert!(bare.size.width > named.size.width, "{bare:?} {named:?}");
}
