use super::*;

fn star(pane: &str, favorite: bool) -> Option<Change> {
    Some(Change {
        pane: pane.into(),
        favorite,
    })
}

#[test]
fn with_no_daemon_the_file_takes_each_change() {
    let dir = tempfile::tempdir().unwrap();
    let state_dir = dir.path().join("state");

    assert_eq!(exchange(&state_dir, None).unwrap(), Favorites::default());
    assert!(!state_dir.exists(), "reading leaves nothing behind");

    exchange(&state_dir, star("p1", true)).unwrap();
    let stood = exchange(&state_dir, star("p2", true)).unwrap();
    assert_eq!(stood.panes, ["p1", "p2"]);
    assert_eq!(load(&file(&state_dir)), stood);

    let stood = exchange(&state_dir, star("p1", false)).unwrap();
    assert_eq!(stood.panes, ["p2"]);
    assert_eq!(exchange(&state_dir, None).unwrap(), stood);
}

#[test]
fn a_pane_that_is_no_id_is_refused_and_saves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let error = exchange(dir.path(), star(" ", true)).unwrap_err();
    assert!(matches!(
        error,
        crate::app::Error::Favorites(Error::Pane { .. })
    ));
    assert!(!file(dir.path()).exists());
}

#[test]
fn a_damaged_file_costs_only_the_favorites() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(file(dir.path()), b"not json").unwrap();
    assert_eq!(exchange(dir.path(), None).unwrap(), Favorites::default());
}
