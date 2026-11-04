use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

#[test]
fn document_toolbar_has_no_english_leaks() {
    under_en_xa("document link toolbar", |cx, recorder| {
        cx.update(|cx| crate::doc_link::Startup::default().install(cx));
        let doc = open("", cx);
        doc.cx.update(|_, cx| {
            doc.editor
                .update(cx, |editor, cx| editor.set_link_supported(true, cx))
        });
        draw(doc.cx);
        assert_translated(
            "document toolbar",
            &uncut(recorder.take()),
            &["todo.md".into()],
        );
    });
}
