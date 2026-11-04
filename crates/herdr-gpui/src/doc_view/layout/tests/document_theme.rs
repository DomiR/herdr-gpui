use super::*;
use crate::{config::DocumentTheme, doc_view::theme};
use gpui::rgba;

const SAMPLE: &str = concat!(
    "# Plan\n",
    "- [x] done é🎉 [docs](https://example.com) `code` **bold**\n",
    "- [ ] next `code`\n",
    "ordinary `code`\n> quote\n---\n```rust\nlet a = 1;\n```",
);

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
fn app_runs_match_the_existing_look_and_style_formulas() {
    let legacy = look();
    assert_eq!(legacy.inks, None);
    let app = themed(DocumentTheme::App);
    let lines = markdown::lines(SAMPLE);
    for decorate in [false, true] {
        for line in &lines {
            let before = row(SAMPLE, line, &legacy, decorate, &[], &|_| None);
            let after = row(SAMPLE, line, &app, decorate, &[], &|_| None);
            assert_eq!(after.runs, before.runs);
            assert_eq!(after.size, before.size);
            assert_eq!(after.line_height, before.line_height);
        }
    }
    let heading = row(SAMPLE, &lines[0], &app, false, &[], &|_| None);
    let title = run_at(&heading, "Plan");
    assert_eq!(title.color, rgb(app.theme.foreground).into());
    assert_eq!(title.font.weight, FontWeight::BOLD);
    assert_eq!(title.background_color, None);
    let done = row(SAMPLE, &lines[1], &app, false, &[], &|_| None);
    let body = run_at(&done, "done");
    assert_eq!(body.color, rgb(app.theme.muted).into());
    assert_eq!(body.font.style, FontStyle::Normal);
    assert_eq!(body.strikethrough.unwrap().color, Some(body.color));
    let link = run_at(&done, "docs");
    assert_eq!(link.color, rgb(app.theme.primary()).into());
    assert_eq!(link.underline.unwrap().color, Some(link.color));
    assert_eq!(link.strikethrough.unwrap().color, Some(link.color));
    assert_eq!(
        run_at(&done, "code").background_color,
        Some(rgb(app.theme.surface).into())
    );
}

#[test]
fn notebook_runs_use_roles_without_changing_source_mapping() {
    let app = themed(DocumentTheme::App);
    let notebook = themed(DocumentTheme::PlainTasks);
    let inks = notebook.inks.unwrap();
    let lines = markdown::lines(SAMPLE);
    for decorate in [false, true] {
        for line in &lines {
            let before = row(SAMPLE, line, &app, decorate, &[], &|_| None);
            let after = row(SAMPLE, line, &notebook, decorate, &[], &|_| None);
            if decorate && line.is_heading() {
                assert_eq!(after.text, &SAMPLE[line.range.clone()]);
            } else {
                assert_eq!(after.text, before.text);
                assert_eq!(after.map, before.map);
            }
            assert_eq!(after.checkbox, before.checkbox);
            assert_eq!(after.links, before.links);
            if !line.is_heading() {
                assert_eq!(after.size, before.size);
                assert_eq!(after.line_height, before.line_height);
            }
            let mut end = 0;
            for run in &after.runs {
                end += run.len;
                assert!(after.text.is_char_boundary(end));
            }
            assert_eq!(end, after.text.len());
        }
        let heading = row(SAMPLE, &lines[0], &notebook, decorate, &[], &|_| None);
        assert_eq!(heading.size, notebook.ui.size);
        assert_eq!(heading.line_height, notebook.ui.size * 1.6);
        assert_eq!(run_at(&heading, "#").color, inks.plain);
        assert_eq!(run_at(&heading, "Plan").color, inks.plain);
        assert_eq!(run_at(&heading, "Plan").font.weight, FontWeight::NORMAL);
        assert_eq!(run_at(&heading, "Plan").background_color, None);
        let done = row(SAMPLE, &lines[1], &notebook, decorate, &[], &|_| None);
        for needle in ["done", "code", "bold"] {
            let run = run_at(&done, needle);
            assert_eq!(run.font.style, FontStyle::Italic);
            assert_eq!(run.strikethrough.unwrap().color, Some(inks.done));
            assert_eq!(run.color, inks.done);
        }
        let link = run_at(&done, "docs");
        assert_eq!(link.color, inks.url);
        assert_eq!(link.font.weight, FontWeight::BOLD);
        assert_eq!(link.font.style, FontStyle::Normal);
        assert_eq!(link.underline.unwrap().color, Some(inks.url));
        assert_eq!(link.strikethrough.unwrap().color, Some(inks.done));
        let open = row(SAMPLE, &lines[2], &notebook, decorate, &[], &|_| None);
        assert_eq!(run_at(&open, "next").color, inks.pending);
        let prose = row(SAMPLE, &lines[3], &notebook, decorate, &[], &|_| None);
        assert_eq!(run_at(&prose, "ordinary").color, inks.plain);
    }
    let bold = row(SAMPLE, &lines[1], &notebook, false, &[], &|_| None);
    assert_eq!(run_at(&bold, "**").color, inks.delimiter);
    for (index, needle) in [(4, ">"), (5, "---"), (6, "```")] {
        let styled = row(SAMPLE, &lines[index], &notebook, false, &[], &|_| None);
        assert_eq!(run_at(&styled, needle).color, rgba(0x00000044).into());
    }
}
