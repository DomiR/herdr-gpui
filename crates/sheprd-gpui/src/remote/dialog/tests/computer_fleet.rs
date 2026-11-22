//! Another computer's claim that asked something of the fleet: the line
//! the approval shows, and the fleet an Allow agrees to only when its
//! switch showed ticked.

use super::*;
use crate::{
    fleet::Fleet,
    remote::dialog::computer_fleet::{Line, line},
};
use sheprd_remote_control::control::fleet::{
    Kind, MemberView, View,
    bluetooth::{Asked, Hint},
};

const OURS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const THEIRS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn member_view(fleet: &str) -> View {
    View {
        fleet: fleet.into(),
        me: "me".into(),
        members: vec![MemberView {
            id: "me".into(),
            name: "studio".into(),
            kind: Kind::Computer,
            admitted_by: None,
        }],
        hosts: Vec::new(),
        removed: 0,
        equivocating: Vec::new(),
        suspended: Vec::new(),
        pending: 0,
        heads: Vec::new(),
        hosted: None,
    }
}

fn invite(fleet: &str) -> Asked {
    Asked::Invite(Hint {
        fleet: fleet.into(),
        computers: 2,
        hosts: 5,
    })
}

#[test]
fn the_line_follows_both_fleets() {
    let ours = member_view(OURS);
    let join = Asked::Join { fleet: OURS.into() };
    assert_eq!(
        Line::of(&join, Some(&ours), Some(OURS)),
        Some(Line::Add { fleet: OURS.into() })
    );
    // A join for a fleet this computer is no longer a member of pairs only.
    assert_eq!(Line::of(&join, Some(&ours), None), None);
    assert_eq!(Line::of(&join, None, None), None);
    assert_eq!(
        Line::of(&invite(THEIRS), None, None),
        Some(Line::Join {
            fleet: THEIRS.into()
        })
    );
    assert_eq!(
        Line::of(&invite(OURS), Some(&ours), Some(OURS)),
        Some(Line::Same)
    );
    assert_eq!(
        Line::of(&invite(THEIRS), Some(&ours), Some(OURS)),
        Some(Line::Other)
    );
    // Holding a fleet as a shared host, or removed, is still another fleet.
    assert_eq!(
        Line::of(&invite(THEIRS), Some(&ours), None),
        Some(Line::Other)
    );
    assert_eq!(Line::Same.agrees(), None);
    assert_eq!(Line::Other.agrees(), None);
}

fn claimed(fleet: Asked) -> Response {
    Response::Pairing(PairingState::Claimed {
        device_name: "Laptop".into(),
        code: "428913".into(),
        app: None,
        ssh: true,
        fleet: Some(fleet),
    })
}

fn approved() -> Response {
    Response::Pairing(PairingState::Approved {
        device: DeviceId("d1".into()),
    })
}

static JOIN_LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn joining_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    JOIN_LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => claimed(Asked::Join { fleet: OURS.into() }),
        Request::Approve { allow: true, .. } => approved(),
        _ => Response::Done,
    })
}

fn approval(log: &Mutex<Vec<Request>>) -> Option<Option<String>> {
    log.lock()
        .unwrap()
        .iter()
        .find_map(|request| match request {
            Request::Approve {
                allow: true, fleet, ..
            } => Some(fleet.clone()),
            _ => None,
        })
}

#[gpui::test]
fn allowing_with_the_switch_ticked_admits_the_computer_to_this_fleet(cx: &mut TestAppContext) {
    JOIN_LOG.lock().unwrap().clear();
    let (view, cx) = window(cx, true, joining_daemon);
    cx.update(|_, cx| {
        Fleet::install(cx);
        Fleet::set_view(cx, Some(member_view(OURS)));
    });
    open(&view, cx);
    wait(cx);
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-computer-fleet").is_some());
    let shown = cx.update(|_, cx| {
        let view = view.read(cx);
        line(
            view.menu.remote_pairing.as_ref().unwrap().pairing.phase(),
            cx,
        )
    });
    assert_eq!(shown, Some(Line::Add { fleet: OURS.into() }));
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    assert_eq!(approval(&JOIN_LOG), Some(Some(OURS.into())));
}

static UNTICKED_LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn unticked_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    UNTICKED_LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => claimed(Asked::Join { fleet: OURS.into() }),
        Request::Approve { allow: true, .. } => approved(),
        _ => Response::Done,
    })
}

#[gpui::test]
fn unticking_the_switch_pairs_only(cx: &mut TestAppContext) {
    UNTICKED_LOG.lock().unwrap().clear();
    let (view, cx) = window(cx, true, unticked_daemon);
    cx.update(|_, cx| {
        Fleet::install(cx);
        Fleet::set_view(cx, Some(member_view(OURS)));
    });
    open(&view, cx);
    wait(cx);
    draw(cx);
    let switch = cx
        .debug_bounds("remote-pair-computer-fleet")
        .unwrap()
        .center();
    cx.simulate_click(switch, Modifiers::default());
    draw(cx);
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    assert_eq!(approval(&UNTICKED_LOG), Some(None));
}

static OTHER_LOG: Mutex<Vec<Request>> = Mutex::new(Vec::new());

fn other_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    OTHER_LOG.lock().unwrap().push(request.clone());
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => claimed(invite(THEIRS)),
        Request::Approve { allow: true, .. } => approved(),
        _ => Response::Done,
    })
}

#[gpui::test]
fn a_computer_in_another_fleet_is_told_and_pairs_only(cx: &mut TestAppContext) {
    OTHER_LOG.lock().unwrap().clear();
    let (view, cx) = window(cx, true, other_daemon);
    cx.update(|_, cx| {
        Fleet::install(cx);
        Fleet::set_view(cx, Some(member_view(OURS)));
    });
    open(&view, cx);
    wait(cx);
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-computer-fleet").is_none());
    assert!(cx.debug_bounds("remote-pair-computer-fleet-note").is_some());
    let allow = cx.debug_bounds("remote-pair-allow").unwrap().center();
    cx.simulate_click(allow, Modifiers::default());
    wait(cx);
    assert_eq!(approval(&OTHER_LOG), Some(None));
}
