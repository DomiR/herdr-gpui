//! Copy link under the QR: one click asks the daemon once, the link lands
//! on the clipboard, and a computer without relays says why instead.

use super::*;

fn linking_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        Request::PairLink { .. } => Response::PairLink {
            link: "sheprd-pair:abc".into(),
            expires_in_ms: 120_000,
        },
        _ => Response::Done,
    })
}

fn relayless_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::PairLink { .. } => Err(control::Error::Daemon {
            code: control::ErrorCode::NoRelays,
            message: "this computer listens on no relay".into(),
        }),
        _ => linking_daemon(Path::new(""), request),
    }
}

fn click_copy(cx: &mut VisualTestContext) {
    draw(cx);
    let copy = cx.debug_bounds("remote-pair-copy-link").unwrap().center();
    cx.simulate_click(copy, Modifiers::default());
    wait(cx);
    draw(cx);
}

#[gpui::test]
fn copy_link_puts_the_daemons_link_on_the_clipboard(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, linking_daemon);
    open(&view, cx);
    wait(cx);
    click_copy(cx);
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("sheprd-pair:abc"));
    assert!(matches!(phase(&view, cx), Phase::Showing { .. }));
    assert!(cx.debug_bounds("remote-pair-link-note").is_some());
}

#[gpui::test]
fn a_computer_without_relays_copies_nothing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, relayless_daemon);
    open(&view, cx);
    wait(cx);
    click_copy(cx);
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), None);
    assert!(matches!(phase(&view, cx), Phase::Showing { .. }));
    let link = view.read_with(cx, |view, _| {
        view.menu
            .remote_pairing
            .as_ref()
            .unwrap()
            .pairing
            .link()
            .clone()
    });
    assert_eq!(link, sheprd_remote_control::app::pairing::Link::NoRelays);
}
