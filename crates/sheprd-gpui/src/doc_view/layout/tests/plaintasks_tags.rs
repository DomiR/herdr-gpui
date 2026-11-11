use super::*;
use crate::{config::DocumentTheme, doc_view::theme};
use gpui::{FontStyle, FontWeight};

fn themed(choice: DocumentTheme) -> Look {
    let app = look();
    let (theme, inks) = theme::resolve(choice, &app.theme);
    Look { theme, inks, ..app }
}

fn run_at<'a>(row: &'a Row, needle: &str) -> &'a TextRun {
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
fn tags_are_bold_and_use_their_tag_ink_and_background() {
    let look = themed(DocumentTheme::PlainTasks);
    let inks = look.inks.unwrap();
    let text = "- [ ] ship @today @other";
    let line = markdown::lines(text).remove(0);
    let styled = row(text, &line, &look, false, &[], &|_| None);
    let today = run_at(&styled, "@today");
    assert_eq!(today.color, inks.today.0);
    assert_eq!(today.font.weight, FontWeight::BOLD);
    assert_eq!(today.font.style, FontStyle::Normal);
    assert_eq!(today.background_color, Some(inks.today.1));
    assert_eq!(run_at(&styled, "@other").color, inks.tag);
    assert_eq!(run_at(&styled, "@other").background_color, None);
}

#[test]
fn done_and_cancelled_tags_use_their_plain_tasks_roles() {
    let look = themed(DocumentTheme::PlainTasks);
    let inks = look.inks.unwrap();
    let text = "- [ ] ship @done(26-11-04 10:10) @cancelled @due(fri)";
    let line = markdown::lines(text).remove(0);
    let styled = row(text, &line, &look, false, &[], &|_| None);
    let done = run_at(&styled, "@done");
    assert_eq!(done.color, inks.tag_done);
    assert_eq!(done.font.weight, FontWeight::BOLD);
    assert_eq!(done.background_color, None);
    let cancelled = run_at(&styled, "@cancelled");
    assert_eq!(cancelled.color, inks.tag_meta);
    assert_eq!(cancelled.font.weight, FontWeight::NORMAL);
    assert_eq!(cancelled.background_color, None);
    let due = run_at(&styled, "@due");
    assert_eq!(due.color, inks.tag);
    assert_eq!(due.font.weight, FontWeight::BOLD);
}

#[test]
fn resolved_task_tags_are_meta_ink_without_bold_or_strike() {
    let look = themed(DocumentTheme::PlainTasks);
    let inks = look.inks.unwrap();
    for text in [
        "- [x] shipped @done(26-11-04 10:10) @today",
        "- [-] cancelled @today",
    ] {
        let line = markdown::lines(text).remove(0);
        let styled = row(text, &line, &look, false, &[], &|_| None);
        for needle in if text.contains("@done") {
            vec!["@done", "@today"]
        } else {
            vec!["@today"]
        } {
            let tag = run_at(&styled, needle);
            assert_eq!(tag.color, inks.tag_meta);
            assert_eq!(tag.font.weight, FontWeight::NORMAL);
            assert_eq!(tag.font.style, FontStyle::Normal);
            assert_eq!(tag.background_color, None);
            assert_eq!(tag.strikethrough, None);
        }
    }
}

#[test]
fn app_theme_does_not_gain_tag_styling() {
    let text = "- [ ] ship @today @done";
    let line = markdown::lines(text).remove(0);
    let app = themed(DocumentTheme::App);
    let legacy = look();
    let themed = row(text, &line, &app, false, &[], &|_| None);
    let unchanged = row(text, &line, &legacy, false, &[], &|_| None);
    assert_eq!(themed.text, unchanged.text);
    assert_eq!(themed.runs, unchanged.runs);
    let tag = run_at(&themed, "@today");
    assert_eq!(tag.color, Hsla::from(rgb(legacy.theme.foreground)));
    assert_eq!(tag.font.weight, FontWeight::NORMAL);
    assert_eq!(tag.background_color, None);
    assert_eq!(
        run_at(&themed, "@done").color,
        Hsla::from(rgb(legacy.theme.foreground))
    );
}
