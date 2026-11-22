#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::tabs::ConsoleLevel;

fn refused(response: Response) -> (ErrorCode, String) {
    match response {
        Response::Error { code, message } => (code, message),
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn values_and_undefined_become_eval_replies() {
    assert_eq!(
        evaluated(3, Ok(Outcome::Undefined)),
        Response::Tab {
            reply: Reply::Eval { value: None }
        }
    );
    let value = serde_json::json!({ "ok": true });
    assert_eq!(
        evaluated(
            3,
            Ok(Outcome::Value {
                value: value.clone()
            })
        ),
        Response::Tab {
            reply: Reply::Eval { value: Some(value) }
        }
    );
}

#[test]
fn a_thrown_or_failed_script_is_refused_with_its_message() {
    let (code, message) = refused(evaluated(
        3,
        Ok(Outcome::Threw {
            message: "TypeError: x is undefined".into(),
        }),
    ));
    assert_eq!(code, ErrorCode::Failed);
    assert_eq!(message, "Tab 3's script threw TypeError: x is undefined");
    let (code, message) = refused(evaluated(
        3,
        Err(Error::Script {
            message: "SyntaxError: Unexpected token ')'".into(),
        }),
    ));
    assert_eq!(code, ErrorCode::Failed);
    assert!(message.ends_with("SyntaxError: Unexpected token ')'"));
}

#[test]
fn a_page_not_yet_loaded_is_not_an_answer_of_its_own() {
    let (code, _) = refused(evaluated(3, Ok(Outcome::NotReady)));
    assert_eq!(code, ErrorCode::NotFound);
    let (code, _) = refused(console(3, Ok(Outcome::NotReady)));
    assert_eq!(code, ErrorCode::NotFound);
}

#[test]
fn a_value_too_large_for_an_answer_is_refused() {
    let value = serde_json::Value::String("x".repeat(MAX_TEXT_JSON));
    let (code, message) = refused(evaluated(3, Ok(Outcome::Value { value })));
    assert_eq!(code, ErrorCode::Failed);
    assert!(message.contains("Return less"));
}

fn kept(entries: serde_json::Value, dropped: u64) -> Result<Outcome, Error> {
    Ok(Outcome::Value {
        value: serde_json::json!({ "entries": entries, "dropped": dropped }),
    })
}

#[test]
fn console_entries_come_back_oldest_first_without_escapes() {
    let response = console(
        5,
        kept(
            serde_json::json!([
                { "level": "log", "text": "ready", "at": 1 },
                { "level": "error", "text": "bad\u{1b}[31m\nline\ttab", "at": 2 },
            ]),
            4,
        ),
    );
    let Response::Tab {
        reply: Reply::Logs { entries, dropped },
    } = response
    else {
        panic!("expected logs, got {response:?}");
    };
    assert_eq!(dropped, 4);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, ConsoleLevel::Log);
    assert_eq!(entries[1].text, "bad\u{fffd}[31m\nline\ttab");
}

#[test]
fn only_the_newest_entries_that_fit_are_kept() {
    let long = "y".repeat(MAX_TEXT_JSON / 3);
    let entries: Vec<_> = (0..5)
        .map(|at| serde_json::json!({ "level": "warn", "text": long, "at": at }))
        .collect();
    let Response::Tab {
        reply: Reply::Logs { entries, dropped },
    } = console(5, kept(serde_json::Value::Array(entries), 1))
    else {
        panic!("expected logs");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries.iter().map(|entry| entry.at).collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(dropped, 4);
}

#[test]
fn a_page_without_a_console_or_with_a_forged_one_is_refused() {
    let (code, _) = refused(console(
        5,
        Ok(Outcome::Value {
            value: serde_json::Value::Null,
        }),
    ));
    assert_eq!(code, ErrorCode::NotFound);
    let (code, _) = refused(console(
        5,
        kept(
            serde_json::json!([{ "level": "shout", "text": "x", "at": 1 }]),
            0,
        ),
    ));
    assert_eq!(code, ErrorCode::Failed);
}
