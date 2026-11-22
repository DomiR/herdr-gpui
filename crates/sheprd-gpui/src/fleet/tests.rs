use super::*;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

mod pairing_error;
mod scrub;
mod settle;

mod plain;

fn link(standing: Standing, ago: Duration, synced: &[&str]) -> (Link, Instant) {
    let now = Instant::now() + Duration::from_secs(7200);
    let link = Link {
        heads: Vec::new(),
        synced: synced.iter().map(|head| (*head).to_owned()).collect(),
        last: Some(now - ago),
        running: false,
        host: None,
        standing,
    };
    (link, now)
}

#[test]
fn a_member_syncs_when_this_computer_moved_or_now_and_then() {
    let mine = vec!["h2".to_owned()];
    let (fresh, now) = link(Standing::Member, Duration::from_secs(5), &["h2"]);
    assert!(!fresh.due(&mine, now));
    let (moved, now) = link(Standing::Member, Duration::from_secs(5), &["h1"]);
    assert!(moved.due(&mine, now));
    let (stale, now) = link(Standing::Member, REFRESH, &["h2"]);
    assert!(stale.due(&mine, now));
}

#[test]
fn machines_outside_the_fleet_are_asked_rarely() {
    let mine = vec!["h1".to_owned()];
    let (outside, now) = link(Standing::Outside, Duration::from_secs(600), &[]);
    assert!(!outside.due(&mine, now));
    let (outside, now) = link(Standing::Outside, OUTSIDE, &[]);
    assert!(outside.due(&mine, now));
    let (failed, now) = link(Standing::Failed, RETRY, &[]);
    assert!(failed.due(&mine, now));
    let (mut running, now) = link(Standing::Unknown, OUTSIDE, &[]);
    running.running = true;
    assert!(!running.due(&mine, now));
    assert!(Link::default().due(&mine, now));
}

#[test]
fn machines_show_their_target() {
    let ssh = ConnectTarget::Ssh {
        target: "me@build-01".into(),
        session: "default".into(),
    };
    assert_eq!(label("id", &ssh), "me@build-01");
    assert_eq!(label("id", &ConnectTarget::Local), "id");
}

#[test]
fn a_fleet_host_keeps_one_saved_device_id() {
    let host = "0123456789abcdef0123456789ABCDEF0123456789abcdef0123456789abcdef";
    assert_eq!(
        saved::device_id(host).as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    assert_eq!(saved::device_id("short"), None);
}

#[test]
fn an_answer_reads_or_says_why_not() {
    let applied = remote::parse_applied(br#"{"fleet":"f","heads":["h"],"ops":[]}"#).unwrap();
    assert_eq!(applied.fleet, "f");
    assert!(matches!(
        remote::parse_applied(b"nope"),
        Err(Error::Answer(_))
    ));
}

#[test]
fn an_approval_at_pairing_is_used_once_and_expires() {
    use sheprd_remote_control::control::{DeviceId, fleet::Kind, fleet::sync::Join};
    let join = |device: &str| Join {
        member: device.into(),
        name: "phone".into(),
        kind: Kind::Phone,
        bundle: "b".into(),
        signature: "s".into(),
        device: "iPhone".into(),
        device_id: DeviceId(device.into()),
        received_ms: 1,
    };
    let now = Instant::now() + Duration::from_secs(7200);
    let mut expected = HashMap::from([
        (
            DeviceId("fresh".into()),
            ("f1".into(), now - Duration::from_secs(5)),
        ),
        (DeviceId("old".into()), ("f1".into(), now - joins::EXPECT)),
        // Given while this computer was in another fleet.
        (DeviceId("elsewhere".into()), ("f0".into(), now)),
    ]);
    let (admit, rest) = joins::settle(
        &mut expected,
        "f1",
        vec![join("fresh"), join("old"), join("elsewhere"), join("other")],
        now,
    );
    assert_eq!(admit, vec![join("fresh")]);
    assert_eq!(rest, vec![join("old"), join("elsewhere"), join("other")]);
    assert!(expected.is_empty());
}

fn fleet_view(heads: &[&str]) -> View {
    View {
        fleet: "f".repeat(64),
        me: "a".into(),
        members: Vec::new(),
        hosts: Vec::new(),
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: heads.iter().map(|head| (*head).to_owned()).collect(),
        hosted: None,
    }
}

#[test]
fn an_older_read_never_replaces_a_newer_view() {
    let mut fleet = Fleet {
        reads: 2,
        asking: true,
        ..Fleet::default()
    };
    // The second read answered first; the first answers late.
    assert!(fleet.answered(2, Ok(Some(fleet_view(&["new"])))));
    assert!(!fleet.answered(1, Ok(Some(fleet_view(&["old"])))));
    assert!(!fleet.answered(1, Ok(None)));
    assert_eq!(fleet.view, Some(fleet_view(&["new"])));
    assert!(!fleet.asking);
}

fn host(id: &str, target: &str) -> sheprd_client::SavedHost {
    sheprd_client::SavedHost {
        id: id.into(),
        label: id.into(),
        target: target.into(),
        session: "default".into(),
        enabled: true,
    }
}

#[test]
fn a_fleet_host_whose_route_moved_is_saved_again_not_forgotten() {
    let before = BTreeSet::from(["moved".to_owned(), "same".to_owned(), "gone".to_owned()]);
    let saved_now = [
        host("moved", "me@r-old.sheprd.invalid"),
        host("same", "me@r-same.sheprd.invalid"),
        host("gone", "me@r-gone.sheprd.invalid"),
        host("mine", "me@build-01"),
    ];
    let wanted = [
        host("moved", "me@r-new.sheprd.invalid"),
        host("same", "me@r-same.sheprd.invalid"),
        host("new", "me@r-fresh.sheprd.invalid"),
        // A person saved this id: it is left alone.
        host("mine", "me@r-other.sheprd.invalid"),
    ];
    let plan = saved::plan(&before, &saved_now, &wanted);
    assert_eq!(plan.forget, vec!["moved".to_owned(), "gone".to_owned()]);
    assert_eq!(
        plan.save,
        vec![
            host("moved", "me@r-new.sheprd.invalid"),
            host("new", "me@r-fresh.sheprd.invalid"),
        ]
    );
    assert_eq!(
        plan.kept,
        BTreeSet::from(["moved".to_owned(), "new".to_owned(), "same".to_owned()])
    );
}

#[test]
fn an_older_update_of_the_saved_devices_never_runs_after_a_newer_one() {
    let saving = saved::Saving::default();
    let ran = std::cell::RefCell::new(Vec::new());
    saving.run(2, || ran.borrow_mut().push(2));
    saving.run(1, || ran.borrow_mut().push(1));
    saving.run(3, || ran.borrow_mut().push(3));
    assert_eq!(*ran.borrow(), vec![2, 3]);
}

#[test]
fn leaving_the_fleet_forgets_every_device_it_saved() {
    let before = BTreeSet::from(["one".to_owned(), "two".to_owned()]);
    let saved_now = [
        host("one", "me@r-one.sheprd.invalid"),
        host("two", "me@r-two.sheprd.invalid"),
        host("mine", "me@build-01"),
    ];
    // In no fleet: nothing is wanted.
    let wanted = saved::wanted(None);
    assert!(wanted.is_empty());
    let plan = saved::plan(&before, &saved_now, &wanted);
    assert_eq!(plan.forget, vec!["one".to_owned(), "two".to_owned()]);
    assert!(plan.save.is_empty() && plan.kept.is_empty());
}

#[test]
fn the_first_answer_of_no_fleet_counts_as_a_change() {
    let mut fleet = Fleet {
        reads: 1,
        ..Fleet::default()
    };
    // A failure is no answer.
    assert!(!fleet.answered(1, Err(Some("refused".into()))));
    fleet.reads = 2;
    assert!(fleet.answered(2, Ok(None)));
    fleet.reads = 3;
    assert!(!fleet.answered(3, Ok(None)));
}

#[test]
fn a_removed_host_that_holds_its_removal_is_never_synced_again() {
    let mine = vec!["h1".to_owned()];
    let (removed, now) = link(Standing::Removed, OUTSIDE * 100, &[]);
    assert!(!removed.due(&mine, now));
}

#[test]
fn a_removed_host_shared_again_or_another_fleet_is_synced_again() {
    let mut view = fleet_view(&["h"]);
    let mut fleet = Fleet {
        view: Some(view.clone()),
        reads: 1,
        ..Fleet::default()
    };
    let (mut removed, _) = link(Standing::Removed, Duration::ZERO, &[]);
    removed.host = Some("host-1".into());
    fleet.links.insert("m1".into(), removed.clone());
    // Still removed: the stop stands.
    fleet.answered(1, Ok(Some(view.clone())));
    assert_eq!(fleet.links["m1"].standing, Standing::Removed);
    // Shared again: synced again.
    view.hosts
        .push(sheprd_remote_control::control::fleet::HostView {
            id: "host-1".into(),
            name: "build-01".into(),
            kind: sheprd_remote_control::control::fleet::HostKind::Sheprd,
            user: "me".into(),
            added_by: "a".into(),
            grants: Vec::new(),
            relays: Vec::new(),
            target: None,
            rev: 1,
            held: false,
            flagged: false,
        });
    fleet.reads = 2;
    fleet.answered(2, Ok(Some(view)));
    assert_eq!(fleet.links["m1"].standing, Standing::Unknown);
    // Another fleet: nothing a machine said of the old one holds.
    let (member, _) = link(Standing::Member, Duration::ZERO, &["h"]);
    fleet.links.insert("m2".into(), member);
    let mut other = fleet_view(&["x"]);
    other.fleet = "e".repeat(64);
    fleet.reads = 3;
    fleet.answered(3, Ok(Some(other)));
    assert_eq!(fleet.links["m2"].standing, Standing::Unknown);
    assert!(Link::default().due(&[], Instant::now()));
}

#[test]
fn a_successful_read_clears_an_earlier_failure() {
    let mut fleet = Fleet {
        reads: 1,
        ..Fleet::default()
    };
    fleet.answered(1, Err(Some("refused".into())));
    assert_eq!(fleet.error.as_deref(), Some("refused"));
    fleet.reads = 2;
    fleet.answered(2, Ok(None));
    assert_eq!(fleet.error, None);
}
