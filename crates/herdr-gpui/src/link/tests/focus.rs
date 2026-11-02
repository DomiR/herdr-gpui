use super::*;

#[cfg(windows)]
const CHECKOUT: &str = r"C:\Users\me\src\herdr-gpui";
#[cfg(not(windows))]
const CHECKOUT: &str = "/home/me/src/herdr-gpui";

fn focus() -> Focus {
    Focus {
        path: Some(CHECKOUT.into()),
        workspace: Some("Notes".into()),
        tab_or_agent: Some(Below::Tab("build".into())),
        id: Some("w3:t2".into()),
        device: Some("my-box".into()),
    }
}

#[test]
fn focus_links_round_trip_in_stable_order() {
    let link = Link::Focus(focus());
    let url = link.to_url();
    let order = ["path=", "workspace=", "tab=", "id=", "device="];
    let positions: Vec<_> = order.iter().map(|key| url.find(key).unwrap()).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{url}");
    assert_eq!(Link::parse(&url).unwrap(), link);
}

#[test]
fn focus_links_accept_agent_targets() {
    let link = Link::Focus(Focus {
        path: None,
        workspace: None,
        tab_or_agent: Some(Below::Agent("reviewer".into())),
        id: None,
        device: None,
    });
    assert_eq!(Link::parse(&link.to_url()).unwrap(), link);
}

#[test]
fn focus_links_require_a_target() {
    assert_eq!(
        Link::parse("herdr-gpui://focus?id=w3%3At2"),
        Err(Error::FocusTargetMissing)
    );
    assert_eq!(
        Link::parse("herdr-gpui://focus?path=/tmp&tab=build&agent=reviewer"),
        Err(Error::FocusTabAndAgent)
    );
}

#[test]
fn focus_links_reject_invalid_values() {
    let cases = [
        (
            "herdr-gpui://focus?workspace=",
            Error::FocusText("workspace"),
        ),
        (
            &format!("herdr-gpui://focus?agent={}", "a".repeat(257)),
            Error::FocusText("agent"),
        ),
        (
            "herdr-gpui://focus?path=/tmp&tab=%0Abuild",
            Error::FocusText("tab"),
        ),
        ("herdr-gpui://focus?path=/tmp&id=", Error::FocusId),
        (
            "herdr-gpui://focus?path=/tmp&device=a%20b",
            Error::FocusDevice,
        ),
        (
            "herdr-gpui://focus?path=relative&workspace=Notes",
            Error::Path,
        ),
    ];
    for (url, error) in cases {
        assert_eq!(Link::parse(url), Err(error), "{url}");
    }
}

#[test]
fn focus_links_reject_unknown_duplicate_and_oversize_parameters() {
    assert_eq!(
        Link::parse("herdr-gpui://focus?path=/tmp&unknown=value"),
        Err(Error::UnknownParameter("unknown".into()))
    );
    assert_eq!(
        Link::parse("herdr-gpui://focus?path=/tmp&path=/other"),
        Err(Error::DuplicateParameter("path"))
    );
    assert_eq!(
        Link::parse(&format!(
            "herdr-gpui://focus?workspace={}",
            "a".repeat(4100)
        )),
        Err(Error::TooLong)
    );
}
