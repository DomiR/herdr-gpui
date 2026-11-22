//! A phone's linked device stays the fleet's when `fleet/links.json` never
//! learned of it: the daemon stopped, or the write failed, between adding
//! the device and recording it. A removal must still reach it.

use super::phones::{admit, apply, device, link, nowhere};
use super::*;
use crate::auth::NewDevice;
use crate::control::fleet::Reason;
use crate::web::tests::fleet::{Phone, device_key};

#[tokio::test]
async fn a_device_the_file_never_named_still_goes_with_its_member() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    apply(&a);
    assert!(device(&a, 20).is_some());
    // The device was added, and the file that says so never written.
    std::fs::remove_file(a.dir.path().join("fleet/links.json")).unwrap();
    // Nothing changes while the link stands, and the file is written again.
    assert!(!apply(&a));
    assert!(device(&a, 20).is_some());
    assert!(a.dir.path().join("fleet/links.json").exists());
    std::fs::remove_file(a.dir.path().join("fleet/links.json")).unwrap();
    a.ask(Ask::Remove {
        member: phone.bundle.id().to_string(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Lost,
    });
    assert!(apply(&a));
    assert!(device(&a, 20).is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revocation_that_failed_to_save_is_retried() {
    let (http, _, _) = relay().await;
    let mut a = Node::new(http, 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    a.start().await;
    let phone = Phone::new(7);
    admit(&a, &phone);
    link(&a, &phone, 20);
    until(10, || device(&a, 20).is_some()).await;
    // `devices.json` cannot be replaced: a directory stands there, so the
    // revocation's save fails and the device is put back.
    let path = a.dir.path().join("devices.json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("blocker"), b"").unwrap();
    a.ask(Ask::Remove {
        member: phone.bundle.id().to_string(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Lost,
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(device(&a, 20).is_some());
    // Freed, with no fleet change: the next try revokes it.
    std::fs::remove_dir_all(&path).unwrap();
    until(10, || device(&a, 20).is_none()).await;
}

#[tokio::test]
async fn a_device_only_named_like_the_fleets_is_not_taken() {
    let a = Node::new(nowhere(), 1).await;
    assert_eq!(a.ask(Ask::Found), Answer::Done);
    // Its id looks like a fleet device's, but another computer vouched for
    // it: not this module's to remove.
    let (id, public_key) = links::device_id(&device_key(30)).unwrap();
    a.state
        .auth()
        .devices_mut()
        .add_with(
            NewDevice {
                app: None,
                fleet: None,
                id: id.clone(),
                name: "iPhone".into(),
                public_key,
                relay: None,
                ssh: None,
            },
            now_ms(),
            Some("studio".into()),
            Vec::new(),
            true,
        )
        .unwrap();
    apply(&a);
    assert!(a.state.auth().devices().get(&id).is_some());
}
