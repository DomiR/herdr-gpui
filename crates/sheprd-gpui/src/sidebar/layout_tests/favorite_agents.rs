use super::*;
use crate::{agent_favorites::Sets, preferences::AgentSort, state::ConnectionStatus};

/// A favorite agent is listed first whatever the sort, under a heading and
/// over a rule, with a star; the agent shortcuts step through the rows in
/// the order they are painted.
#[gpui::test]
fn favorites_are_painted_first_in_every_sort(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            let mut listing = snapshot(1);
            listing.agents.truncate(1);
            listing.agents[0].agent_status = AgentStatus::Idle;
            view.live.snapshot = Some(Arc::new(listing.clone()));
            view.live.status = ConnectionStatus::Connected;
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            listing.agents[0].agent_status = AgentStatus::Blocked;
            remote.live.snapshot = Some(Arc::new(listing));
            remote.live.status = ConnectionStatus::Connected;
            view.endpoints.push(remote);
            // The idle local agent is starred; the same pane on the remote
            // host is another agent, and is not.
            view.agent_favorites = Arc::new(Sets::starred(&[(crate::endpoint::LOCAL, &["p0"])]));
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    let (local, remote) = ("agent-local-p0", "agent-ssh:test-p0");
    for sort in [AgentSort::Grouped, AgentSort::Priority] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.agent_sort = sort;
                cx.notify();
            });
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        let heading = cx.debug_bounds("header-agent-favorites").unwrap();
        let starred = cx.debug_bounds(local).unwrap();
        let rule = cx.debug_bounds("agent-favorites-rule").unwrap();
        let other = cx.debug_bounds(remote).unwrap();
        assert!(heading.top() < starred.top(), "{sort:?}");
        assert!(starred.bottom() <= rule.top(), "{sort:?}");
        assert!(rule.bottom() <= other.top(), "{sort:?}");
        assert!(cx.debug_bounds("agent-star-local-p0").is_some(), "{sort:?}");
        assert!(
            cx.debug_bounds("agent-star-ssh:test-p0").is_none(),
            "{sort:?}"
        );
        cx.update(|_, cx| {
            let stepped: Vec<_> = view
                .read(cx)
                .sidebar_agents()
                .into_iter()
                .map(|(host, _)| host)
                .collect();
            assert_eq!(stepped, [0, 1], "{sort:?}");
        });
    }
}
