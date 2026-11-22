//! `fleet scrub` takes back the API access the fleet's phone links gave:
//! every device a link admitted goes, found by `fleet/links.json` or by
//! its mark, so a damaged file still lets them go.

use super::phones::{admit, apply, device, link, nowhere};
use super::*;
use crate::web::tests::fleet::Phone;

/// `a` founds a fleet and admits a phone whose link made a device here.
async fn linked() -> (Node, Phone) {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    assert!(apply(&a));
    assert!(device(&a, 20).is_some());
    (a, phone)
}

#[tokio::test]
async fn a_scrub_revokes_the_phones_the_fleet_linked() {
    let (a, _phone) = linked().await;
    assert_eq!(a.ask(Ask::Scrub { session: None }), Answer::Done);
    assert!(device(&a, 20).is_none());
    assert!(!a.fleet.state_dir().join("fleet/links.json").exists());
    // Gone from disk too, as a restarted daemon reads it.
    let auth =
        crate::auth::Auth::load(a.fleet.state_dir(), crate::auth::Limits::default()).unwrap();
    let (id, _) = links::device_id(&crate::web::tests::fleet::device_key(20)).unwrap();
    assert!(auth.devices().get(&id).is_none());
}

#[tokio::test]
async fn a_damaged_links_file_still_lets_the_phones_go() {
    let (a, _phone) = linked().await;
    let file = a.fleet.state_dir().join("fleet/links.json");
    std::fs::write(&file, b"not json").unwrap();
    // As the offline scrub does: the devices read from disk.
    let mut auth =
        crate::auth::Auth::load(a.fleet.state_dir(), crate::auth::Limits::default()).unwrap();
    links::forget(&mut auth, a.fleet.state_dir()).unwrap();
    let (id, _) = links::device_id(&crate::web::tests::fleet::device_key(20)).unwrap();
    assert!(auth.devices().get(&id).is_none());
    assert!(!file.exists());
}

#[tokio::test]
async fn links_applied_after_a_scrub_admit_nothing() {
    let (a, _phone) = linked().await;
    assert_eq!(a.ask(Ask::Scrub { session: None }), Answer::Done);
    // The follower runs after the scrub: no fleet, so nothing comes back.
    assert!(!apply(&a));
    assert!(device(&a, 20).is_none());
}
