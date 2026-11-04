use super::*;
use crate::{config::DocumentTheme, doc_view::theme};
use gpui::{FontStyle, FontWeight};

fn plaintasks() -> Look {
    let app = look();
    let (theme, inks) = theme::resolve(DocumentTheme::PlainTasks, &app.theme);
    Look { theme, inks, ..app }
}

fn run_at<'a>(row: &'a Row, needle: &str) -> &'a gpui::TextRun {
    let at = row.text.find(needle).unwrap();
    let mut end = 0;
    row.runs
        .iter()
        .find(|run| {
            end += run.len;
            end > at
        })
        .unwrap()
}

#[test]
fn headings_keep_body_size_and_their_marker_visible() {
    let look = plaintasks();
    let inks = look.inks.unwrap();
    let text = "# Plan";
    let line = markdown::lines(text).remove(0);
    let row = row(text, &line, &look, true, &[], &|_| None);
    assert_eq!(row.text.as_ref(), text);
    assert_eq!(row.size, look.ui.size);
    assert_eq!(row.line_height, look.ui.size * 1.6);
    assert_eq!(run_at(&row, "#").color, inks.plain);
    assert_eq!(run_at(&row, "Plan").font.weight, FontWeight::NORMAL);
}

#[test]
fn cancelled_text_is_italic_without_strike_and_done_text_keeps_strike() {
    let look = plaintasks();
    let inks = look.inks.unwrap();
    let cancelled_text = "- [-] cancelled";
    let cancelled_line = markdown::lines(cancelled_text).remove(0);
    let cancelled = row(
        cancelled_text,
        &cancelled_line,
        &look,
        false,
        &[],
        &|_| None,
    );
    let cancelled_run = run_at(&cancelled, "cancelled");
    assert_eq!(cancelled_run.color, inks.cancelled);
    assert_eq!(cancelled_run.font.style, FontStyle::Italic);
    assert_eq!(cancelled_run.strikethrough, None);

    let done_text = "- [x] done";
    let done_line = markdown::lines(done_text).remove(0);
    let done = row(done_text, &done_line, &look, false, &[], &|_| None);
    let done_run = run_at(&done, "done");
    assert_eq!(done_run.color, inks.done);
    assert_eq!(done_run.font.style, FontStyle::Italic);
    assert!(done_run.strikethrough.is_some());
}

#[test]
fn delimiters_links_paragraphs_and_quotes_use_plain_tasks_roles() {
    let look = plaintasks();
    let inks = look.inks.unwrap();
    let text = "**bold** *italic* `code` [link](https://example.test)";
    let line = markdown::lines(text).remove(0);
    let row = row(text, &line, &look, false, &[], &|_| None);
    assert_eq!(run_at(&row, "**").color, inks.delimiter);
    assert_eq!(run_at(&row, "*").color, inks.delimiter);
    assert_eq!(run_at(&row, "`").color, inks.delimiter);
    let link = run_at(&row, "link");
    assert_eq!(link.color, inks.url);
    assert_eq!(link.font.weight, FontWeight::BOLD);
    assert!(link.underline.is_some());

    let paragraph = row("paragraph", &markdown::lines("paragraph").remove(0), &look, false, &[], &|_| None);
    assert_eq!(paragraph.runs[0].color, inks.plain);
    let quote_text = "> quote";
    let quote = row(
        quote_text,
        &markdown::lines(quote_text).remove(0),
        &look,
        false,
        &[],
        &|_| None,
    );
    assert_eq!(run_at(&quote, "quote").color, inks.notes);
    assert_eq!(run_at(&quote, "> ").color, inks.separator);
}
