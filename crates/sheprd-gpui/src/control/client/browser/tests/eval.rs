#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::tabs::ConsoleLevel;

fn refusal(error: crate::Error) -> (ErrorCode, String) {
    match error {
        crate::Error::Control(crate::control::Error::Rejected { code, message }) => (code, message),
        other => panic!("unexpected error {other:?}"),
    }
}

#[test]
fn scripts_come_from_the_argument_or_standard_input() {
    let unused = std::io::empty();
    assert_eq!(
        script_text(ScriptSource::Inline("document.title".into()), unused).unwrap(),
        "document.title"
    );
    assert_eq!(
        script_text(ScriptSource::Stdin, "return 1\n".as_bytes()).unwrap(),
        "return 1\n"
    );
}

#[test]
fn empty_and_oversized_scripts_are_refused_before_sending() {
    let (code, message) =
        refusal(script_text(ScriptSource::Inline(" \n".into()), std::io::empty()).unwrap_err());
    assert_eq!(code, ErrorCode::InvalidRequest);
    assert_eq!(message, "The script is empty");
    let long = "x".repeat(MAX_EVAL_SCRIPT_BYTES + 10);
    let (code, _) = refusal(script_text(ScriptSource::Stdin, long.as_bytes()).unwrap_err());
    assert_eq!(code, ErrorCode::InvalidRequest);
    let fits = "x".repeat(MAX_EVAL_SCRIPT_BYTES);
    assert_eq!(
        script_text(ScriptSource::Stdin, fits.as_bytes())
            .unwrap()
            .len(),
        MAX_EVAL_SCRIPT_BYTES
    );
}

#[test]
fn values_print_as_text_or_json() {
    let value = |value| Response::Tab {
        reply: Reply::Eval { value },
    };
    let text = evaluated(2, value(Some(serde_json::json!("Kontor")))).unwrap();
    assert_eq!(text.text, "Kontor");
    assert_eq!(text.json, serde_json::json!({ "id": 2, "value": "Kontor" }));
    let object = evaluated(2, value(Some(serde_json::json!({ "a": 1 })))).unwrap();
    assert_eq!(object.text, "{\n  \"a\": 1\n}");
    let undefined = evaluated(2, value(None)).unwrap();
    assert!(undefined.text.starts_with("undefined"));
    assert_eq!(
        undefined.json,
        serde_json::json!({ "id": 2, "value": null })
    );
    assert!(
        evaluated(
            2,
            Response::Tab {
                reply: Reply::Text { text: "x".into() }
            }
        )
        .is_err()
    );
}

#[test]
fn console_entries_print_one_a_line() {
    let logs = |entries, dropped| Response::Tab {
        reply: Reply::Logs { entries, dropped },
    };
    let entry = |level, text: &str| ConsoleEntry {
        level,
        text: text.into(),
        at: 1,
    };
    let output = console(
        7,
        logs(
            vec![
                entry(ConsoleLevel::Log, "ready"),
                entry(ConsoleLevel::Error, "boom"),
            ],
            3,
        ),
    )
    .unwrap();
    assert_eq!(
        output.text,
        "(3 older entries were dropped)\nlog   ready\nerror boom"
    );
    assert_eq!(output.json["dropped"], 3);
    assert_eq!(output.json["entries"][1]["level"], "error");
    let empty = console(7, logs(Vec::new(), 0)).unwrap();
    assert_eq!(empty.status, crate::control::client::exit::NOTHING);
}
