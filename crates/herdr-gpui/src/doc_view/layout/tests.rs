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
    row(text, &lines[0], &look(), false, &[])
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

#[test]
fn decorated_lines_hide_markers_and_map_back_to_the_source() {
    let text = "## Plan **now**";
    let lines = markdown::lines(text);
    let row = row(text, &lines[0], &look(), true, &[]);
    assert_eq!(row.text.as_ref(), "Plan now");
    assert_eq!(
        row.runs.iter().map(|run| run.len).sum::<usize>(),
        "Plan now".len()
    );
    let map = &row.map;
    // "P" is drawn first, after the hidden "## ".
    assert_eq!(map.to_display(3), 0);
    assert_eq!(map.to_source(0), 3);
    // Inside a hidden marker, the caret is drawn where it was hidden.
    assert_eq!(map.to_display(1), 0);
    // "now" starts after the hidden "**".
    assert_eq!(map.to_display(10), 5);
    assert_eq!(map.to_source(5), 10);
    assert_eq!(map.to_display(text.len()), "Plan now".len());
    assert_eq!(map.to_source("Plan now".len()), text.len());
}

#[test]
fn bullets_draw_as_dots_and_a_task_box_keeps_its_place() {
    let text = "- a\n- [x] b";
    let lines = markdown::lines(text);
    let bullet = row(text, &lines[0], &look(), true, &[]);
    assert_eq!(bullet.text.as_ref(), "• a");
    assert_eq!(bullet.map.to_source("• ".len()), 2);
    let task = row(text, &lines[1], &look(), true, &[]);
    assert_eq!(task.text.as_ref(), "[x] b");
    assert_eq!(task.checkbox, Some((0..3, true)));
}

#[test]
fn code_lines_take_their_token_colours() {
    use crate::review::highlight::{Span, Token};
    let text = "```rust\nfn main() {}\n```";
    let lines = markdown::lines(text);
    let spans = [Span {
        start: 0,
        end: 2,
        token: Token::Keyword,
    }];
    let plain = row(text, &lines[1], &look(), false, &[]);
    let coloured = row(text, &lines[1], &look(), false, &spans);
    assert_ne!(plain.runs[0].color, coloured.runs[0].color);
    assert_eq!(coloured.runs[0].len, 2);
}
