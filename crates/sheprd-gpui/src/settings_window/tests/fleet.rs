//! Settings > Fleet: the page shows what the daemon keeps, and nothing is
//! signed until a person confirms here; removing a compromised member
//! takes what it added with it.
use super::remote_control::{click, draw};
use super::*;
use crate::{
    config::RemoteConfig,
    fleet::Fleet,
    remote::{Health, Supervisor},
};
use sheprd_remote_control::control::{
    self, Request, Response,
    fleet::{
        HostKind, HostView, Kind, MemberView, Reason, View,
        sync::{Answer, Ask},
    },
};
use std::path::Path;

pub(super) static ASKED: Mutex<Vec<Ask>> = Mutex::new(Vec::new());
/// The stand-in daemon is shared: these tests take turns.
static SERIAL: Mutex<()> = Mutex::new(());
static IN_FLEET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// The fleet removed this computer.
static REMOVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// This computer holds the log as a shared host, not as a member.
static HOSTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The joins the daemon lists.
static JOINS: Mutex<Vec<control::fleet::sync::Join>> = Mutex::new(Vec::new());

mod joins;

fn view() -> View {
    let member = |id: &str, name: &str, kind, by: Option<&str>| MemberView {
        id: id.into(),
        name: name.into(),
        kind,
        admitted_by: by.map(str::to_owned),
    };
    let host = |id: &str, name: &str, kind, by: &str, grants: &[&str]| HostView {
        id: id.into(),
        name: name.into(),
        kind,
        user: "me".into(),
        added_by: by.into(),
        grants: grants.iter().map(|grant| (*grant).to_owned()).collect(),
        relays: Vec::new(),
        target: None,
        rev: 1,
        held: false,
        flagged: false,
    };
    View {
        fleet: "f".repeat(64),
        me: "a".into(),
        members: vec![
            member("a", "studio", Kind::Computer, None),
            member("b", "laptop", Kind::Computer, Some("a")),
            member("c", "spare", Kind::Computer, Some("b")),
            member("p", "iPhone", Kind::Phone, Some("a")),
        ],
        hosts: vec![
            host("ha", "studio", HostKind::Member, "a", &["b", "c"]),
            host("hx", "build-01", HostKind::Agent, "b", &["a", "b", "c"]),
        ],
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: vec!["h".into()],
        hosted: None,
    }
}

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    let Request::Fleet(ask) = request else {
        return Ok(Response::Done);
    };
    let answer = match ask.as_ref() {
        Ask::Status => Answer::Status {
            view: IN_FLEET.load(Ordering::SeqCst).then(|| {
                let mut view = view();
                if HOSTED.load(Ordering::SeqCst) {
                    view.members.retain(|member| member.id != view.me);
                    view.hosted = Some("ha".into());
                }
                if REMOVED.load(Ordering::SeqCst) {
                    view.members.retain(|member| member.id != view.me);
                }
                Box::new(view)
            }),
        },
        Ask::Joins => Answer::Joins {
            joins: JOINS.lock().unwrap().clone(),
        },
        ask => {
            // The daemon drops a join once it is admitted.
            if let Ask::Admit { bundle, .. } = ask {
                JOINS.lock().unwrap().retain(|join| join.bundle != *bundle);
            }
            ASKED.lock().unwrap().push(ask.clone());
            Answer::Done
        }
    };
    Ok(Response::Fleet(Box::new(answer)))
}

pub(super) fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn page(cx: &mut TestAppContext, in_fleet: bool) -> &mut VisualTestContext {
    IN_FLEET.store(in_fleet, Ordering::SeqCst);
    REMOVED.store(false, Ordering::SeqCst);
    HOSTED.store(false, Ordering::SeqCst);
    ASKED.lock().unwrap().clear();
    JOINS.lock().unwrap().clear();
    let (view, cx) = cx.add_window_view(skill_fixture);
    let config = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    cx.update(|_, cx| {
        Supervisor::fixture(cx, config, daemon);
        Supervisor::set_health(cx, Health::Running(remote_control::status()), None);
        Fleet::install(cx);
    });
    view.update_in(cx, |view, window, cx| {
        view.select_section(Section::Fleet, window, cx)
    });
    cx.simulate_resize(size(px(960.), px(2400.)));
    cx.run_until_parked();
    draw(cx);
    cx
}

#[gpui::test]
fn the_page_lists_members_hosts_and_who_may_reach_each(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    for selector in [
        "fleet-member-0",
        "fleet-member-3",
        "fleet-member-1-remove",
        "fleet-host-0",
        "fleet-host-1",
        "fleet-host-1-remove",
        "fleet-host-1-member-0-toggle",
        "fleet-machines-none",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    // A computer's own host goes with it, from Members.
    assert!(cx.debug_bounds("fleet-host-0-remove").is_none());
    assert!(cx.debug_bounds("fleet-create").is_none());
    assert!(ASKED.lock().unwrap().is_empty());
}

#[gpui::test]
fn removing_a_compromised_member_asks_why_then_takes_what_it_added(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    click(cx, "fleet-member-1-remove");
    assert!(
        ASKED.lock().unwrap().is_empty(),
        "nothing is signed on the first click"
    );
    // Lost, Compromised, Retired.
    click(cx, "fleet-member-1-reason-1");
    assert!(ASKED.lock().unwrap().is_empty());
    click(cx, "fleet-member-1-confirm");
    cx.run_until_parked();
    assert_eq!(
        *ASKED.lock().unwrap(),
        vec![Ask::Remove {
            member: "b".into(),
            also_members: vec!["c".into()],
            also_hosts: vec!["hx".into()],
            reason: Reason::Compromised,
        }]
    );
}

#[gpui::test]
fn a_host_is_removed_after_a_second_click_and_access_toggles_at_once(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    click(cx, "fleet-host-1-remove");
    assert!(ASKED.lock().unwrap().is_empty());
    click(cx, "fleet-host-1-remove");
    cx.run_until_parked();
    click(cx, "fleet-host-1-member-0-toggle");
    cx.run_until_parked();
    assert_eq!(
        *ASKED.lock().unwrap(),
        vec![
            Ask::RemoveHost { host: "hx".into() },
            Ask::Access {
                host: "hx".into(),
                member: "a".into(),
                allow: false,
            },
        ]
    );
}

#[gpui::test]
fn with_no_fleet_the_page_offers_to_create_one(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    assert!(cx.debug_bounds("fleet-member-0").is_none());
    click(cx, "fleet-create");
    cx.run_until_parked();
    assert_eq!(*ASKED.lock().unwrap(), vec![Ask::Found]);
}

#[gpui::test]
fn a_computer_the_fleet_removed_forgets_it_when_asked(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    REMOVED.store(true, Ordering::SeqCst);
    cx.update(|_, cx| Fleet::refresh(cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("fleet-member-0").is_none());
    click(cx, "fleet-forget");
    cx.run_until_parked();
    assert_eq!(*ASKED.lock().unwrap(), vec![Ask::Scrub { session: None }]);
}

#[gpui::test]
fn with_no_fleet_a_machine_still_offers_to_remove_fleet_access(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, false);
    let reachable = std::collections::BTreeMap::from([(
        "build".to_owned(),
        crate::device_tags::Daemon::Remote(sheprd_client::ConnectTarget::Ssh {
            target: "me@build-01".into(),
            session: "default".into(),
        }),
    )]);
    cx.update(|_, cx| Fleet::reach(cx, EntityId::from(1_u64), &reachable));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("fleet-create").is_some());
    // As `sheprd fleet scrub build` opens it: asked here first.
    click(cx, "fleet-machine-0-scrub");
    assert!(cx.debug_bounds("fleet-machine-0-scrub-confirm").is_some());
    click(cx, "fleet-machine-0-scrub-cancel");
    assert!(cx.debug_bounds("fleet-machine-0-scrub-confirm").is_none());
    assert!(ASKED.lock().unwrap().is_empty());
}

#[gpui::test]
fn a_shared_host_is_not_shown_as_removed(cx: &mut TestAppContext) {
    let _serial = serial();
    let cx = page(cx, true);
    HOSTED.store(true, Ordering::SeqCst);
    cx.update(|_, cx| Fleet::refresh(cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("fleet-hosted").is_some());
    assert!(cx.debug_bounds("fleet-removed").is_none());
    assert!(cx.debug_bounds("fleet-forget").is_none());
    // The members show, with nothing to sign.
    assert!(cx.debug_bounds("fleet-member-0").is_some());
    assert!(cx.debug_bounds("fleet-member-1-remove").is_none());
    HOSTED.store(false, Ordering::SeqCst);
}
