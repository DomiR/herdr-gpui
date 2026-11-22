use super::*;

fn change(pane: &str, favorite: bool) -> Change {
    Change {
        pane: pane.into(),
        favorite,
    }
}

fn favorites(panes: &[&str]) -> Favorites {
    Favorites {
        boot: None,
        panes: panes.iter().map(|pane| (*pane).to_owned()).collect(),
    }
}

#[test]
fn favorites_belong_to_their_device() {
    let sets = Sets::starred(&[("local", &["p1"]), ("studio", &["p2"])]);
    assert!(sets.contains("local", "p1"));
    assert!(
        !sets.contains("local", "p2"),
        "pane ids repeat across devices"
    );
    assert!(sets.contains("studio", "p2"));
    assert!(!sets.contains("away", "p1"));
    assert!(sets.any());
    assert!(!Sets::default().any());
    assert!(!Sets::starred(&[("local", &[])]).any());
}

#[test]
fn an_answer_replaces_a_devices_favorites_and_no_daemon_clears_them() {
    let mut sets = Sets::default();
    assert!(sets.replace("local", Some(favorites(&["p1"]))));
    assert!(!sets.replace("local", Some(favorites(&["p1"]))));
    assert!(sets.replace("local", None));
    assert!(!sets.contains("local", "p1"));
    assert!(!sets.replace("local", None));
}

#[test]
fn changes_still_on_their_way_are_laid_over_an_answer() {
    let pending = [change("p2", true), change("p1", false)];
    let shown = with_pending(favorites(&["p1", "p3"]), &pending);
    assert_eq!(shown.panes, ["p3", "p2"]);
    assert_eq!(with_pending(favorites(&["p1"]), &[]).panes, ["p1"]);
}

#[test]
fn a_host_with_no_daemon_prints_nothing_and_bad_answers_are_refused() {
    assert!(sync::parse(b"").unwrap().is_none());
    assert!(sync::parse(b" \n").unwrap().is_none());
    assert!(matches!(sync::parse(b"usage"), Err(Error::Answer(_))));
    let answer = sync::parse(br#"{"panes":["p1","p1",""]}"#)
        .unwrap()
        .unwrap();
    assert_eq!(answer.panes, ["p1"], "bounded as the daemon bounds them");
}

#[gpui::test]
fn a_star_shows_at_once_and_waits_for_its_daemon(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        AgentFavorites::install(cx);
        AgentFavorites::set(cx, "studio", "p1".into(), true);
        assert!(AgentFavorites::current(cx).contains("studio", "p1"));
        let link = &cx.global::<AgentFavorites>().links["studio"];
        assert_eq!(link.pending, [change("p1", true)], "no daemon reached yet");

        AgentFavorites::set(cx, "studio", " ".into(), true);
        assert_eq!(
            cx.global::<AgentFavorites>().links["studio"].pending.len(),
            1,
            "a pane that is no id is not sent"
        );

        // The daemon answers before taking a second star sent meanwhile.
        AgentFavorites::set(cx, "studio", "p2".into(), true);
        let first = change("p1", true);
        AgentFavorites::answered(cx, "studio", Some(&first), Ok(Some(favorites(&["p1"]))));
        let sets = AgentFavorites::current(cx);
        assert!(sets.contains("studio", "p1") && sets.contains("studio", "p2"));
        assert_eq!(
            cx.global::<AgentFavorites>().links["studio"].pending,
            [change("p2", true)]
        );

        AgentFavorites::answered(cx, "studio", None, Ok(None));
        assert!(!AgentFavorites::current(cx).contains("studio", "p1"));
        assert!(
            cx.global::<AgentFavorites>().links["studio"]
                .pending
                .is_empty()
        );
    });
}

#[test]
fn a_failed_exchange_waits_for_the_interval_but_a_new_change_goes_at_once() {
    let interval = Duration::from_secs(15);
    let mut link = Link {
        last: Some(Instant::now()),
        ..Link::default()
    };
    assert!(!link.due(interval), "read just now");
    link.pending.push_back(change("p1", true));
    assert!(link.due(interval), "a change waiting goes at once");
    link.failed = true;
    assert!(!link.due(interval), "after a failure, not on the next poll");
    link.last = Instant::now().checked_sub(interval);
    assert!(link.due(interval), "but once the interval passed");
    link.running = true;
    assert!(!link.due(interval), "never twice at once");
}
