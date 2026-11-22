use super::*;

#[test]
fn an_app_icon_is_sheprd_s_own() {
    let own = Icons.load("icons/sessions.svg").unwrap().unwrap();
    let app = HostIcons.load("icons/app-sessions.svg").unwrap().unwrap();
    assert_eq!(own, app);
    // The rest are Sheprd's assets as before.
    assert_eq!(HostIcons.load("icons/sessions.svg").unwrap().unwrap(), own);
    assert!(matches!(
        HostIcons.load("icons/app-no-such.svg"),
        Ok(None) | Err(_)
    ));
}

#[test]
fn an_icon_s_app_name_is_its_file_s() {
    assert_eq!(
        app_name("icons/pull-request.svg").as_deref(),
        Some("app-pull-request")
    );
    assert_eq!(app_name("agents/claude.svg"), None);
}

#[test]
fn app_icons_are_listed_beside_the_others() {
    let listed = HostIcons.list("icons/").unwrap();
    assert!(
        listed
            .iter()
            .any(|name| name.as_ref() == "icons/sessions.svg")
    );
    assert!(
        listed
            .iter()
            .any(|name| name.as_ref() == "icons/app-sessions.svg")
    );
    let apps = HostIcons.list("icons/app-").unwrap();
    assert!(!apps.is_empty());
    assert!(apps.iter().all(|name| name.starts_with("icons/app-")));
}
