#![allow(clippy::unwrap_used)]
use super::*;
use crate::config::Config;

fn look() -> Look {
    Look {
        theme: Theme::default(),
        ui: Config::default().ui,
        code: Config::default().terminal,
    }
}

fn styled(text: &str) -> Row {
    let lines = markdown::lines(text);
    row(text, &lines[0], &look())
}

#[test]
fn runs_cover_the_line_without_splitting_characters() {
    for text in [
        "- [x] **wörld** `code` [a](https://b.c) é🎉",
        "# Title",
        "",
        "```",
    ] {
        let row = styled(text);
        assert_eq!(
            row.runs.iter().map(|run| run.len).sum::<usize>(),
            text.len(),
            "{text}"
        );
        let mut at = 0;
        for run in &row.runs {
            at += run.len;
            assert!(text.is_char_boundary(at), "{text}");
        }
    }
}

#[test]
fn a_task_box_is_drawn_not_its_brackets() {
    let row = styled("- [x] done");
    assert_eq!(row.checkbox, Some((2..5, true)));
    let first = &row.runs[0];
    assert_eq!(first.len, 2);
    let hidden = &row.runs[1];
    assert_eq!(hidden.len, 3);
    assert_eq!(hidden.color.a, 0.);
    // The done item's text is struck through.
    assert!(row.runs.last().unwrap().strikethrough.is_some());
}

#[test]
fn headings_draw_larger_and_links_are_found() {
    let heading = styled("## Plan");
    assert!(heading.size > look().ui.size);
    let link = styled("see [docs](https://example.com/x)");
    assert_eq!(link.links, vec![(5..9, "https://example.com/x".to_owned())]);
}
