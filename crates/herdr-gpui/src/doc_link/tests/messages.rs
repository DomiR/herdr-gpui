use super::*;

fn range() -> Range {
    Range {
        from: version(3),
        to: version(7),
    }
}

#[test]
fn start_has_the_exact_protocol_text() {
    assert_eq!(
        start(&path(), version(7)),
        "/herdrdocwatch /tmp/notes folder/O'Reilly.md\n\nHERDR_DOC_PUSH v1\nMode: on\nDocument: /tmp/notes folder/O'Reilly.md\nVersion: 7\nHerdr UI pushes this document's settled changes to you as prompts starting with HERDR_DOC_PUSH v1. Do not start doc watch or a debounce script for it, and stop your own watch of it if one runs. Keep the inbox watch."
    );
}

#[test]
fn inline_change_has_the_exact_protocol_text() {
    assert_eq!(
        change(&path(), range(), Some("@@ -1 +1 @@\n-old\n+new")),
        "HERDR_DOC_PUSH v1\nMode: change\nDocument: /tmp/notes folder/O'Reilly.md\nVersions: 3 to 7\nO'Reilly.md changed. Settled net diff below; it can include your own status lines, which are not new work.\n~~~diff\n@@ -1 +1 @@\n-old\n+new\n~~~"
    );
}

#[test]
fn inline_change_strips_the_diff_trailing_newline() {
    let message = change(&path(), range(), Some("@@ -1 +1 @@\n-old\n+new\n"));

    assert_eq!(message.matches("\n\n~~~").count(), 0);
    assert!(message.ends_with("+new\n~~~"));
}

#[test]
fn pointer_change_quotes_one_shell_word() {
    assert_eq!(
        change(&path(), range(), None),
        "HERDR_DOC_PUSH v1\nMode: change\nDocument: /tmp/notes folder/O'Reilly.md\nVersions: 3 to 7\nO'Reilly.md changed, now version 7. Run doc diff '/tmp/notes folder/O'\\''Reilly.md' --since 3. If that fails, run doc read on it instead."
    );
}

#[test]
fn resync_and_off_have_the_exact_protocol_text() {
    assert_eq!(
        resync(&path(), version(7)),
        "HERDR_DOC_PUSH v1\nMode: resync\nDocument: /tmp/notes folder/O'Reilly.md\nVersion: 7\nHerdr UI restarted, so document versions started again. Read the document once and reconcile your saved state with live agents; do not redispatch tasks already assigned. Pushes stay on: do not start a watch."
    );
    assert_eq!(
        off(&path(), version(3), version(7)),
        "HERDR_DOC_PUSH v1\nMode: off\nDocument: /tmp/notes folder/O'Reilly.md\nVersion: 7\nPushes for this document are off. Changes after version 3 were not pushed. To keep following it, start your own doc watch with --since 3."
    );
}

#[test]
fn fence_grows_past_a_run_in_the_diff() {
    let diff = "before\n~~~\nafter";
    let message = change(&path(), range(), Some(diff));
    assert!(message.contains("\n~~~~diff\n"));
    assert!(message.ends_with("\n~~~~"));
}

#[test]
fn inline_edges_are_inclusive_and_empty_diff_is_a_pointer() {
    let bytes = "x".repeat(INLINE_BYTES);
    let lines = (0..INLINE_LINES)
        .map(|_| "x")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(change(&path(), range(), Some(&bytes)).contains("~~~diff"));
    assert!(change(&path(), range(), Some(&lines)).contains("~~~diff"));
    assert!(!change(&path(), range(), Some("")).contains("~~~diff"));
    assert!(!change(&path(), range(), Some(&"x".repeat(INLINE_BYTES + 1))).contains("~~~diff"));
    assert!(
        !change(
            &path(),
            range(),
            Some(
                &(0..INLINE_LINES + 1)
                    .map(|_| "x")
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        )
        .contains("~~~diff")
    );
}
