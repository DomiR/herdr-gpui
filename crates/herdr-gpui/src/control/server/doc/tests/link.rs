#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[gpui::test]
fn a_path_without_a_link_reports_off(cx: &mut TestAppContext) {
    let doc = file("# Plan\n");
    let response = reply(ask(
        DocOp::Link {
            path: doc.path,
            off: false,
        },
        cx,
    ));
    assert!(matches!(
        response,
        Reply::Link {
            status: crate::control::protocol::doc::LinkStatus {
                mode: crate::control::protocol::doc::LinkMode::Off,
                acked: None,
                agent: None,
                pane: None,
                pending: None,
            },
            ..
        }
    ));
}

#[gpui::test]
fn turning_off_a_missing_link_is_idempotent(cx: &mut TestAppContext) {
    let doc = file("# Plan\n");
    let response = reply(ask(
        DocOp::Link {
            path: doc.path,
            off: true,
        },
        cx,
    ));
    assert!(matches!(
        response,
        Reply::Link {
            status: crate::control::protocol::doc::LinkStatus {
                mode: crate::control::protocol::doc::LinkMode::Off,
                ..
            },
            ..
        }
    ));
}
