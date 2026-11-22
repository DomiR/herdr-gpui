#![allow(clippy::unwrap_used)]
use super::super::tests::{setup, window};
use super::super::{Found, Step};
use crate::i18n::fl;
use gpui::TestAppContext;
use sheprd_remote_control::control::desktop::PairOutput;

#[gpui::test]
fn text_that_is_no_link_says_so_and_keeps_the_list(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update_in(cx, |view, _, cx| {
        let input = setup(view).link.clone();
        input.update(cx, |input, cx| {
            input.set_text_at("https://example.org/pair", 0, cx);
        });
        view.pair_from_link(cx);
        assert_eq!(setup(view).step, Step::Looking);
        assert_eq!(
            setup(view).error.as_deref(),
            Some(fl!("error-pairing-link-format").as_str())
        );
    });
}

#[gpui::test]
fn an_empty_field_does_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update_in(cx, |view, _, cx| {
        view.pair_from_link(cx);
        assert_eq!(setup(view).step, Step::Looking);
        assert_eq!(setup(view).error, None);
    });
}

#[gpui::test]
fn a_links_code_shows_once_the_claim_brings_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update_in(cx, |view, window, cx| {
        let studio = Found {
            peripheral: String::new(),
            name: "Studio".into(),
        };
        assert_eq!(studio.bluetooth(), None);
        view.menu.nearby_setup.as_mut().unwrap().step = Step::Exchanging(studio.clone());
        view.nearby_output(
            PairOutput::Linked {
                computer: "Studio".into(),
                fleet: None,
                pairing: "p".into(),
            },
            window,
            cx,
        );
        let showing = |code: &str, claimed| Step::Code {
            computer: studio.clone(),
            code: code.into(),
            claimed,
            offer: super::super::fleet::Offer::None,
            pairing: "p".into(),
            decided: true,
        };
        assert_eq!(setup(view).step, showing("", false));
        // Only six digits are taken as the code.
        view.nearby_output(
            PairOutput::Claimed {
                code: Some("12ab56".into()),
            },
            window,
            cx,
        );
        assert_eq!(setup(view).step, showing("", true));
        view.nearby_output(
            PairOutput::Claimed {
                code: Some("428913".into()),
            },
            window,
            cx,
        );
        assert_eq!(setup(view).step, showing("428913", true));
    });
}

#[gpui::test]
fn add_link_opens_nearby_sheprd_with_the_link_and_pairs_on_no_click(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        // A window on an explicit socket has no Add Device.
        assert!(!view.open_pairing_link("sheprd-pair:abc", window, cx));
        view.menu.page = None;
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Session {
            name: "default".into(),
            development: false,
        };
        assert!(view.open_pairing_link("sheprd-pair:abc", window, cx));
        assert_eq!(view.menu.page, Some(crate::menu::Page::AddNearby));
        let setup = setup(view);
        assert_eq!(setup.link.read(cx).text(), "sheprd-pair:abc");
        // Nothing starts before the user clicks Pair.
        assert_eq!(setup.step, Step::Looking);
        assert!(setup.pairing.is_none());
    });
}
