use super::*;
use crate::{config::DocumentTheme, doc_view::theme};
use gpui::{FontStyle, FontWeight, rgba};

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
fn resolved_task_tags_are_meta_ink_without_bold_or_strike() {
    let look = themed(DocumentTheme::PlainTasks);
    let inks = look.inks.unwrap();
    for text in ["- [x] shipped @today", "- [-] cancelled @today"] {
        let line = markdown::lines(text).remove(0);
        let styled = row(text, &line, &look, false, &[], &|_| None);
        let tag = run_at(&styled, "@today");
        assert_eq!(tag.color, inks.tag_meta);
        assert_eq!(tag.font.weight, FontWeight::NORMAL);
        assert_eq!(tag.font.style, FontStyle::Normal);
        assert_eq!(tag.background_color, None);
        assert_eq!(tag.strikethrough, None);
    }
}

#[test]
fn app_theme_does_not_gain_tag_styling() {
    let text = "- [ ] ship @today";
    let line = markdown::lines(text).remove(0);
    let app = themed(DocumentTheme::App);
    let legacy = look();
    let themed = row(text, &line, &app, false, &[], &|_| None);
    let unchanged = row(text, &line, &legacy, false, &[], &|_| None);
    assert_eq!(themed.text, unchanged.text);
    assert_eq!(themed.runs, unchanged.runs);
    let tag = run_at(&themed, "@today");
    assert_eq!(tag.color, rgba(legacy.theme.foreground).into());
    assert_eq!(tag.font.weight, FontWeight::NORMAL);
    assert_eq!(tag.background_color, None);
}
