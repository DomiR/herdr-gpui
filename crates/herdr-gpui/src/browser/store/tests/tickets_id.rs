use super::*;

#[test]
fn a_saved_tab_never_takes_the_tickets_pages_id() {
    let mut store = Store::default();
    let page = store
        .open(
            Scope::endpoint("local"),
            "w_1",
            url("https://a.test/"),
            None,
        )
        .unwrap();
    let mut tabs = store.tabs.clone();
    let mut reserved = tabs[0].clone();
    reserved.id = TabId::TICKETS;
    tabs.push(reserved);
    let bytes = serde_json::to_vec(&Saved { tabs }).unwrap();
    let mut restored = Store::with_tabs(parse(&bytes).unwrap(), None);
    assert!(restored.get(page).is_some());
    assert!(restored.get(TabId::TICKETS).is_none());
    let next = restored
        .open(Scope::endpoint("local"), "w_1", None, None)
        .unwrap();
    assert_ne!(next, TabId::TICKETS);
}
