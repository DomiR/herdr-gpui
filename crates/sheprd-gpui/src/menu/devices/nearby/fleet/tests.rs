#![allow(clippy::unwrap_used)]
use super::Offer;
use sheprd_remote_control::control::fleet::{Kind, MemberView, View, bluetooth::Hint};

const OURS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const THEIRS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn view() -> View {
    View {
        fleet: OURS.into(),
        me: "me".into(),
        members: vec![MemberView {
            id: "me".into(),
            name: "laptop".into(),
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

fn hint(fleet: &str) -> Hint {
    Hint {
        fleet: fleet.into(),
        computers: 1,
        hosts: 0,
    }
}

#[test]
fn the_offer_follows_both_fleets() {
    let ours = view();
    assert_eq!(Offer::of(Some(&ours), true, Some(&hint(OURS))), Offer::Same);
    assert_eq!(
        Offer::of(Some(&ours), true, Some(&hint(THEIRS))),
        Offer::Other(hint(OURS))
    );
    assert_eq!(
        Offer::of(Some(&ours), true, None),
        Offer::Invite(hint(OURS))
    );
    assert_eq!(
        Offer::of(None, false, Some(&hint(THEIRS))),
        Offer::Join(hint(THEIRS))
    );
    assert_eq!(Offer::of(None, false, None), Offer::Found);
    // A fleet held as a shared host, or after a removal, offers nothing.
    assert_eq!(
        Offer::of(Some(&ours), false, Some(&hint(THEIRS))),
        Offer::None
    );
    assert_eq!(Offer::of(Some(&ours), false, None), Offer::None);
}

#[test]
fn only_a_choice_holds_the_claim_back() {
    assert!(Offer::Invite(hint(OURS)).asks());
    assert!(Offer::Join(hint(THEIRS)).asks());
    assert!(Offer::Found.asks());
    assert!(!Offer::Same.asks());
    assert!(!Offer::Other(hint(OURS)).asks());
    assert!(!Offer::None.asks());
}

mod page {
    use super::{OURS, THEIRS, hint, view};
    use crate::{
        SheprdWindow,
        fleet::Fleet,
        menu::devices::nearby::{Found, NearbySetup, Step, fleet::Offer},
        sidebar::layout_tests::fixture_window,
    };
    use gpui::TestAppContext;
    use sheprd_remote_control::control::{
        desktop::PairOutput,
        fleet::bluetooth::{ClaimFleet, Hint},
    };

    fn exchanging(view: &mut SheprdWindow, cx: &mut gpui::Context<SheprdWindow>) {
        let link = crate::menu::devices::nearby::link::input(view, cx);
        view.menu.nearby_setup = Some(NearbySetup {
            attempt: crate::menu::devices::nearby::next_attempt(),
            found: Vec::new(),
            error: None,
            pairing: None,
            radio: None,
            scanner: None,
            link,
            step: Step::Exchanging(Found {
                peripheral: "p".into(),
                name: "Studio".into(),
            }),
            _poll: None,
        });
    }

    fn code(theirs: Option<Hint>) -> PairOutput {
        PairOutput::Code {
            code: "428913".into(),
            computer: "Studio".into(),
            fleet: theirs,
            pairing: "digest".into(),
        }
    }

    fn step(view: &SheprdWindow) -> (Offer, bool) {
        match &view.menu.nearby_setup.as_ref().unwrap().step {
            Step::Code { offer, decided, .. } => (offer.clone(), *decided),
            other => panic!("{other:?}"),
        }
    }

    #[gpui::test]
    fn a_member_is_asked_before_inviting_and_may_pair_only(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, Some(view_of_member()));
            exchanging(view, cx);
            view.nearby_output(code(None), window, cx);
            assert_eq!(step(view), (Offer::Invite(hint(OURS)), false));
            view.choose_nearby_fleet(false, cx);
            assert_eq!(step(view), (Offer::Invite(hint(OURS)), true));
            // Decided once: a second click changes nothing.
            view.choose_nearby_fleet(true, cx);
            assert!(step(view).1);
        });
    }

    #[gpui::test]
    fn the_same_fleet_or_another_claims_without_asking(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, Some(view_of_member()));
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(OURS))), window, cx);
            assert_eq!(step(view), (Offer::Same, true));
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            assert_eq!(step(view), (Offer::Other(hint(OURS)), true));
        });
    }

    #[gpui::test]
    fn an_answer_for_a_cancelled_attempt_is_dropped(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(Some(hint(THEIRS))), window, cx);
            assert_eq!(step(view), (Offer::Join(hint(THEIRS)), false));
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            view.stop_nearby_pairing(cx);
            // The daemon's bundle arrives after the cancel: nothing moves.
            view.nearby_claim_with(attempt, Ok(ClaimFleet::Invite(hint(OURS))), cx);
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert_eq!(setup.step, Step::Looking);
            assert_eq!(setup.error, None);
        });
    }

    #[gpui::test]
    fn a_daemon_that_cannot_sign_stops_the_pairing(cx: &mut TestAppContext) {
        let (window, cx) = cx.add_window_view(fixture_window);
        window.update_in(cx, |view, window, cx| {
            Fleet::install(cx);
            Fleet::set_view(cx, None);
            exchanging(view, cx);
            view.nearby_output(code(None), window, cx);
            assert_eq!(step(view), (Offer::Found, false));
            let attempt = view.menu.nearby_setup.as_ref().unwrap().attempt;
            view.nearby_claim_with(attempt, Err(crate::fleet::Error::NoDaemon), cx);
            let setup = view.menu.nearby_setup.as_ref().unwrap();
            assert_eq!(setup.step, Step::Looking);
            assert!(setup.error.is_some());
        });
    }

    fn view_of_member() -> sheprd_remote_control::control::fleet::View {
        view()
    }
}
