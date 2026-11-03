use super::*;

fn color(value: u32) -> Hsla {
    rgb(value).into()
}

#[test]
fn paragraph_text_is_prose() {
    let row = styled("plain text");
    assert_eq!(row.runs[0].color, color(look().theme.prose()));
}

#[test]
fn plain_bullets_and_numbers_dim_only_their_text() {
    for text in ["- item", "1. item"] {
        let row = styled(text);
        assert_eq!(row.runs[0].color, color(look().theme.muted));
        assert_eq!(row.runs[1].color, color(look().theme.prose()));
    }
}

#[test]
fn open_task_text_stays_foreground() {
    let row = styled("- [ ] ship it");
    assert_eq!(
        row.runs.last().unwrap().color,
        color(look().theme.foreground)
    );
    assert!(!row
        .runs
        .iter()
        .any(|run| run.color == color(look().theme.prose())));
}

#[test]
fn heading_text_stays_foreground() {
    let row = styled("# Title");
    assert_eq!(row.runs.last().unwrap().color, color(look().theme.foreground));
}

#[test]
fn inline_emphasis_and_code_stay_foreground_inside_prose() {
    let row = styled("plain **bold** and *it* and `code`");
    let theme = look().theme;
    assert_eq!(
        row.runs.iter().map(|run| run.color).collect::<Vec<_>>(),
        vec![
            color(theme.prose()),
            color(theme.muted),
            color(theme.foreground),
            color(theme.muted),
            color(theme.prose()),
            color(theme.muted),
            color(theme.foreground),
            color(theme.muted),
            color(theme.prose()),
            color(theme.muted),
            color(theme.foreground),
            color(theme.muted),
        ]
    );
}

#[test]
fn links_keep_the_primary_color() {
    let row = styled("see [docs](https://example.com)");
    let theme = look().theme;
    assert!(row.runs.iter().any(|run| run.color == color(theme.primary())));
}

#[test]
fn a_note_under_a_task_is_prose() {
    let text = "- [ ] task\n  a note under it";
    let lines = markdown::lines(text);
    let row = row(text, &lines[1], &look(), false, &[], &|_| None);
    assert_eq!(row.runs[0].color, color(look().theme.prose()));
}

#[test]
fn done_task_text_stays_muted() {
    let row = styled("- [x] done");
    assert_eq!(row.runs.last().unwrap().color, color(look().theme.muted));
}

#[test]
fn prose_is_not_foreground() {
    assert_ne!(look().theme.prose(), look().theme.foreground);
}
