use super::*;
use crate::protocol::MAX_WAIT_SECONDS;

#[test]
fn eval_and_logs_round_trip() {
    round_trip(TabOp::Eval {
        id: 3,
        script: "document.title".into(),
        wait_seconds: 15,
    });
    round_trip(TabOp::Logs { id: 3, clear: true });
    round_trip(TabOp::ClearData { id: 3 });
    // `clear` may be left out.
    let op: TabOp = serde_json::from_str(r#"{"op":"logs","id":3}"#).unwrap();
    assert_eq!(
        op,
        TabOp::Logs {
            id: 3,
            clear: false
        }
    );
}

#[test]
fn eval_waits_as_asked_up_to_the_bound() {
    let eval = |wait_seconds| TabOp::Eval {
        id: 1,
        script: "1".into(),
        wait_seconds,
    };
    assert_eq!(eval(40).wait_seconds(), 40);
    assert_eq!(eval(u64::MAX).wait_seconds(), MAX_WAIT_SECONDS);
    assert_eq!(
        TabOp::Logs {
            id: 1,
            clear: false
        }
        .wait_seconds(),
        PAGE_WAIT_SECONDS
    );
}

#[test]
fn replies_keep_values_and_console_entries() {
    for reply in [
        Reply::Eval { value: None },
        Reply::Eval {
            value: Some(serde_json::json!({ "a": [1, null, "x"] })),
        },
        Reply::Eval {
            value: Some(serde_json::Value::Null),
        },
        Reply::Logs {
            entries: vec![ConsoleEntry {
                level: ConsoleLevel::Warn,
                text: "slow".into(),
                at: 1_700_000_000_000,
            }],
            dropped: 2,
        },
    ] {
        let response = Response::Tab {
            reply: reply.clone(),
        };
        let text = serde_json::to_string(&response).unwrap();
        assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
    }
    // `undefined` has no value at all, unlike `null`.
    let undefined = serde_json::to_value(Reply::Eval { value: None }).unwrap();
    assert_eq!(undefined, serde_json::json!({ "kind": "eval" }));
}

#[test]
fn console_levels_are_named_as_json_spells_them() {
    for level in [
        ConsoleLevel::Debug,
        ConsoleLevel::Error,
        ConsoleLevel::Info,
        ConsoleLevel::Log,
        ConsoleLevel::Warn,
    ] {
        assert_eq!(
            serde_json::to_value(level).unwrap(),
            serde_json::Value::String(level.name().into())
        );
    }
}
