//! Page commands leave the user's view alone unless the caller passes
//! `--focus`: no workspace switch, no tab brought forward. Only the refusals
//! run here, since a page that is made needs a native web view.
use super::*;

/// Serves `op` once and returns the answer, which must come at once: a
/// request left waiting for a page would block here.
fn ask(cx: &mut VisualTestContext, op: TabOp) -> Response {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let (call, incoming, waiting) = queued(&server, op, Caller::default());
    let mut watches = Watches::default();
    cx.cx.update(|cx| {
        serve(call, incoming, &mut watches, cx);
        watches.serve(cx);
    });
    waiting.join().unwrap().unwrap()
}

/// The window still shows workspace `w0` and is going nowhere else.
fn stayed(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.read_with(cx, |view, _| {
        assert!(view.shows_workspace(0, "w0"));
        assert!(view.pending_navigation.is_none());
    });
}

#[gpui::test]
fn page_text_for_a_workspace_no_window_lists_switches_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let page = open(&view, cx, "w_gone", Some(web("https://a.test/")));
    let answer = ask(
        cx,
        TabOp::Text {
            id: page.into(),
            focus: false,
        },
    );
    assert!(
        matches!(
            answer,
            Response::Error {
                code: ErrorCode::NotFound,
                ..
            }
        ),
        "{answer:?}"
    );
    stayed(&view, cx);
}

/// WebKit pictures what is on screen, so without `--focus` a page the user
/// does not see is refused rather than brought forward.
#[cfg(target_os = "macos")]
#[gpui::test]
fn a_screenshot_without_focus_pictures_only_a_page_on_screen(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let behind = open(&view, cx, "w0", Some(web("https://a.test/")));
    let elsewhere = open(&view, cx, "w1", Some(web("https://b.test/")));
    let folder = tempfile::tempdir().unwrap();
    for id in [behind, elsewhere] {
        let path = folder.path().join("page.png");
        let answer = ask(
            cx,
            TabOp::Screenshot {
                id: id.into(),
                path: path.to_str().unwrap().into(),
                focus: false,
            },
        );
        assert!(
            matches!(
                answer,
                Response::Error {
                    code: ErrorCode::NotFound,
                    ..
                }
            ),
            "{answer:?}"
        );
        assert!(!path.exists());
        stayed(&view, cx);
    }
}
