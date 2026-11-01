#![allow(clippy::unwrap_used)]
use super::*;

mod fold_regions;

fn kinds(text: &str) -> Vec<Kind> {
    lines(text).into_iter().map(|line| line.kind).collect()
}

#[test]
fn block_kinds_and_markers() {
    let text = "# Title\n\n- [ ] open\n  - [x] done\n1. first\n> quote\n---\n```rust\nlet x;\n```\nplain\n";
    assert_eq!(
        kinds(text),
        vec![
            Kind::Heading { level: 1 },
            Kind::Blank,
            Kind::Item {
                list: List::Bullet('-'),
                task: Some(false)
            },
            Kind::Item {
                list: List::Bullet('-'),
                task: Some(true)
            },
            Kind::Item {
                list: List::Ordered {
                    number: 1,
                    delimiter: '.'
                },
                task: None
            },
            Kind::Quote,
            Kind::Rule,
            Kind::Fence,
            Kind::Code,
            Kind::Fence,
            Kind::Paragraph,
            Kind::Blank,
        ]
    );
    let all = lines(text);
    assert_eq!(&text[all[0].marker.clone()], "# ");
    assert_eq!(&text[all[2].marker.clone()], "- [ ] ");
    assert_eq!(&text[all[2].checkbox.clone().unwrap()], "[ ]");
    assert_eq!(&text[all[3].content()], "done");
    assert_eq!(all[3].depth, 2);
    assert_eq!(&text[all[4].marker.clone()], "1. ");
}

#[test]
fn code_blocks_hide_markdown_inside() {
    assert_eq!(
        kinds("```\n# not a heading\n- [ ] not a task\n```"),
        vec![Kind::Fence, Kind::Code, Kind::Code, Kind::Fence]
    );
}

#[test]
fn near_misses_are_paragraphs() {
    for text in ["#tag", "-dash", "[ ] box", "1.5 pts", "**bold**"] {
        assert_eq!(kinds(text), vec![Kind::Paragraph], "{text}");
    }
    assert_eq!(
        kinds("- [x]done"),
        vec![Kind::Item {
            list: List::Bullet('-'),
            task: None
        }]
    );
}

#[test]
fn crlf_line_breaks_stay_out_of_the_line() {
    let text = "- [ ] a\r\nb\r\n";
    let all = lines(text);
    assert_eq!(&text[all[0].range.clone()], "- [ ] a");
    assert_eq!(&text[all[1].range.clone()], "b");
    assert_eq!(line_at(&all, 9), 1);
}

#[test]
fn inline_styles_with_their_markers() {
    let text = "- [ ] **bold** and *it* `code` ~~gone~~ [site](https://a.test)";
    let all = lines(text);
    let spans = inline(text, &all[0]);
    let styled = |style: Style| -> Vec<&str> {
        spans
            .iter()
            .filter(|span| span.style == style)
            .map(|span| &text[span.range.clone()])
            .collect()
    };
    assert_eq!(styled(Style::Strong), vec!["bold"]);
    assert_eq!(styled(Style::Emphasis), vec!["it"]);
    assert_eq!(styled(Style::Code), vec!["code"]);
    assert_eq!(styled(Style::Strike), vec!["gone"]);
    assert_eq!(
        styled(Style::Marker),
        vec![
            "**",
            "**",
            "*",
            "*",
            "`",
            "`",
            "~~",
            "~~",
            "[",
            "](https://a.test)"
        ]
    );
    let link = spans
        .iter()
        .find(|span| matches!(span.style, Style::Link { .. }))
        .unwrap();
    assert_eq!(&text[link.range.clone()], "site");
    assert_eq!(link_url(text, link), Some("https://a.test"));
}
