//! A prompt's note travels beside its words without breaking clients or
//! saved conversations that predate it.

use super::*;

#[test]
fn a_prompt_without_a_note_reads_and_writes_as_before() {
    let old = r#"{"type":"user","text":"hi"}"#;
    let kind: EntryKind = serde_json::from_str(old).unwrap();
    assert_eq!(
        kind,
        EntryKind::User {
            text: "hi".to_owned(),
            files: 0,
            note: None,
        }
    );
    assert_eq!(serde_json::to_string(&kind).unwrap(), old);
}

#[test]
fn a_prompt_keeps_its_note_apart_from_its_words() {
    let kind = EntryKind::User {
        text: "hi".to_owned(),
        files: 0,
        note: Some("<sheprd sum=\"0\">\nBe brief.\n</sheprd>".to_owned()),
    };
    let json = serde_json::to_value(&kind).unwrap();
    assert_eq!(json["text"], "hi");
    assert_eq!(serde_json::from_value::<EntryKind>(json).unwrap(), kind);
    assert_eq!(kind.text(), Some("hi"));
}
