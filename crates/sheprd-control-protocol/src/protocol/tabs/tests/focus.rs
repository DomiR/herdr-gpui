use super::*;

#[test]
fn page_reads_take_focus_only_when_asked() {
    for op in [
        TabOp::Text { id: 3, focus: true },
        TabOp::Screenshot {
            id: 3,
            path: "/tmp/a.png".into(),
            focus: true,
        },
    ] {
        round_trip(op);
    }
    // Asking for no focus says nothing, so an app older than the flag
    // still takes the request.
    assert_eq!(
        serde_json::to_string(&TabOp::Text {
            id: 3,
            focus: false
        })
        .unwrap(),
        r#"{"op":"text","id":3}"#
    );
    assert_eq!(
        serde_json::to_string(&TabOp::Screenshot {
            id: 3,
            path: "/a.png".into(),
            focus: true
        })
        .unwrap(),
        r#"{"op":"screenshot","id":3,"path":"/a.png","focus":true}"#
    );
}

#[test]
fn a_request_without_focus_stays_in_the_background() {
    let op = |json: &str| serde_json::from_str::<TabOp>(json).unwrap();
    assert_eq!(
        op(r#"{"op":"text","id":3}"#),
        TabOp::Text {
            id: 3,
            focus: false
        }
    );
    assert_eq!(
        op(r#"{"op":"screenshot","id":3,"path":"/a.png"}"#),
        TabOp::Screenshot {
            id: 3,
            path: "/a.png".into(),
            focus: false
        }
    );
    assert!(serde_json::from_str::<TabOp>(r#"{"op":"text","id":3,"focus":"yes"}"#).is_err());
}
