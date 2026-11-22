#![allow(clippy::unwrap_used)]
use super::*;

fn outcome(text: &str) -> Result<Outcome, Error> {
    parse(text)
}

#[test]
fn an_expression_is_returned_and_its_trailing_semicolons_dropped() {
    let source = wrapped("document.title ;\n", Form::Expression);
    assert!(source.starts_with(PRELUDE));
    // Nothing runs on the blank document a page starts on.
    assert!(PRELUDE.starts_with("if (location.href === 'about:blank')"));
    assert!(source.ends_with(EPILOGUE));
    assert!(source.contains("async () => (\ndocument.title\n)"));
}

#[test]
fn a_body_keeps_its_statements() {
    let script = "const a = await f();\nreturn a.b; // done";
    let source = wrapped(script, Form::Body);
    assert!(source.contains(&format!("async () => {{\n{script}\n}}")));
}

#[test]
fn answers_become_outcomes() {
    assert_eq!(
        outcome(r#"{"kind":"undefined"}"#).unwrap(),
        Outcome::Undefined
    );
    assert_eq!(
        outcome(r#"{"kind":"value","value":{"a":[1,null]}}"#).unwrap(),
        Outcome::Value {
            value: serde_json::json!({ "a": [1, null] })
        }
    );
    // JSON drops a function, leaving no value.
    assert_eq!(
        outcome(r#"{"kind":"value"}"#).unwrap(),
        Outcome::Value {
            value: serde_json::Value::Null
        }
    );
    assert_eq!(
        outcome(r#"{"kind":"threw","message":"TypeError: x is undefined"}"#).unwrap(),
        Outcome::Threw {
            message: "TypeError: x is undefined".into()
        }
    );
    assert_eq!(
        outcome(r#"{"kind":"not_ready"}"#).unwrap(),
        Outcome::NotReady
    );
    assert!(matches!(outcome("{}").unwrap_err(), Error::Answer(_)));
    assert!(matches!(outcome("not json").unwrap_err(), Error::Answer(_)));
}

#[test]
fn the_check_carries_both_forms_as_string_literals() {
    let forms = Forms {
        expression: wrapped("a\u{2028}\"b\"", Form::Expression),
        body: wrapped("return 1", Form::Body),
    };
    let check = forms.check().unwrap();
    assert!(check.starts_with(CHECK_PRELUDE));
    assert!(check.ends_with(CHECK_EPILOGUE));
    // The sources are JSON strings, quotes escaped, never raw script.
    assert!(check.contains(&serde_json::to_string(&forms.expression).unwrap()));
    assert!(!check.contains("\"b\""));
    assert_eq!(
        serde_json::from_str::<Checked>(r#"{"kind":"syntax","message":"SyntaxError: x"}"#).unwrap(),
        Checked::Syntax {
            message: "SyntaxError: x".into()
        }
    );
    assert_eq!(
        serde_json::from_str::<Checked>(r#"{"kind":"unchecked"}"#).unwrap(),
        Checked::Unchecked
    );
}

#[test]
fn a_site_record_covers_its_hosts_and_no_others() {
    assert!(belongs("localhost", "localhost"));
    assert!(belongs("127.0.0.1", "127.0.0.1"));
    assert!(belongs("example.com", "app.example.com"));
    assert!(belongs("Example.com", "EXAMPLE.com"));
    assert!(!belongs("example.com", "badexample.com"));
    assert!(!belongs("example.com", "example.org"));
    assert!(!belongs("", "example.com"));
}

#[test]
fn the_console_keeper_is_bounded_and_named_as_reads_expect() {
    assert!(CONSOLE.contains("Symbol.for(\"sheprd.console\")"));
    assert!(READ_CONSOLE.contains("Symbol.for('sheprd.console')"));
    assert!(CONSOLE.contains("var MAX_ENTRIES = 500;"));
}
