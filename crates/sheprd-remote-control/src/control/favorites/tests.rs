use super::*;

fn star(pane: &str) -> Change {
    Change {
        pane: pane.into(),
        favorite: true,
    }
}

fn unstar(pane: &str) -> Change {
    Change {
        pane: pane.into(),
        favorite: false,
    }
}

fn panes(favorites: &Favorites) -> Vec<&str> {
    favorites.panes.iter().map(String::as_str).collect()
}

#[test]
fn a_change_names_one_pane_so_others_stand() {
    let favorites = Favorites::default()
        .changed(star("p1"))
        .unwrap()
        .changed(star("p2"))
        .unwrap();
    assert_eq!(panes(&favorites), ["p1", "p2"]);
    assert!(favorites.contains("p1"));

    let favorites = favorites.changed(unstar("p1")).unwrap();
    assert_eq!(panes(&favorites), ["p2"]);
    assert!(!favorites.contains("p1"));
    assert_eq!(
        favorites.clone().changed(unstar("p1")).unwrap(),
        favorites,
        "unstarring twice changes nothing"
    );
}

#[test]
fn starring_again_makes_it_the_newest() {
    let favorites = Favorites::default()
        .changed(star("p1"))
        .unwrap()
        .changed(star("p2"))
        .unwrap()
        .changed(star("p1"))
        .unwrap();
    assert_eq!(panes(&favorites), ["p2", "p1"]);
}

#[test]
fn panes_that_are_not_ids_are_refused() {
    for pane in ["", "  ", "a\nb", &"p".repeat(MAX_PANE_BYTES + 1)] {
        assert_eq!(Favorites::default().changed(star(pane)), None, "{pane:?}");
    }
    assert!(
        Favorites::default()
            .changed(star(&"p".repeat(MAX_PANE_BYTES)))
            .is_some()
    );
}

#[test]
fn past_the_bound_the_oldest_go() {
    let mut favorites = Favorites::default();
    for index in 0..=MAX_FAVORITES {
        favorites = favorites.changed(star(&format!("p{index}"))).unwrap();
    }
    assert_eq!(favorites.panes.len(), MAX_FAVORITES);
    assert!(!favorites.contains("p0"));
    assert!(favorites.contains(&format!("p{MAX_FAVORITES}")));
}

#[test]
fn a_saved_copy_is_bounded_keeping_the_newest() {
    let saved = Favorites {
        boot: None,
        panes: vec!["a".into(), "".into(), "b".into(), "a".into()],
    };
    assert_eq!(panes(&saved.bounded()), ["b", "a"]);
}

#[test]
fn a_new_primary_boot_drops_only_its_panes() {
    let core = core_pane("core-1", "t1");
    let mut favorites = Favorites {
        boot: None,
        panes: vec!["p1".into(), "herdr:work:p1".into(), core.clone()],
    };
    assert!(favorites.follow_boot("boot-1"), "the first boot is adopted");
    assert_eq!(favorites.panes.len(), 3, "and drops nothing");
    assert!(!favorites.follow_boot("boot-1"));
    assert!(!favorites.follow_boot(""), "an unknown boot is no restart");

    assert!(favorites.follow_boot("boot-2"));
    assert_eq!(panes(&favorites), ["herdr:work:p1", core.as_str()]);
    assert_eq!(favorites.boot.as_deref(), Some("boot-2"));
}

#[test]
fn core_panes_name_their_core_and_keep_addresses_apart() {
    assert_eq!(core_pane("fake-core", "t1"), core_pane("fake-core", "t1"));
    assert_ne!(core_pane("core-a", "t1"), core_pane("core-b", "t1"));
    assert_ne!(core_pane("c", "box/t1"), core_pane("c", "boxt1"));
    assert!(core_pane("c", "box/t1").starts_with(CORE_PREFIX));
    assert!(!core_pane("c", "box/t1").contains('/'));
}

#[test]
fn a_change_reads_as_the_phone_sends_it() {
    let change: Change = serde_json::from_str(r#"{"pane":"p1","favorite":true}"#).unwrap();
    assert_eq!(change, star("p1"));
    assert_eq!(
        serde_json::to_string(&Favorites::default()).unwrap(),
        r#"{"panes":[]}"#
    );
}
