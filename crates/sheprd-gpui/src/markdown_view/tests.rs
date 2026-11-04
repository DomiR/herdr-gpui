#![allow(clippy::unwrap_used)]
use super::{
    parse::{Block, Inline, ListItem, Style},
    *,
};

fn paragraph(text: &str) -> Block {
    Block::Paragraph(Inline {
        text: text.into(),
        spans: Vec::new(),
    })
}

#[test]
fn blocks_follow_the_document() {
    let blocks = parse("# Plan\n\nSome **bold** text.\n\n---\n\n```rust\nfn main() {}\n```\n");
    assert_eq!(blocks.len(), 4);
    assert!(matches!(&blocks[0], Block::Heading(1, inline) if inline.text == "Plan"));
    let Block::Paragraph(inline) = &blocks[1] else {
        panic!("{:?}", blocks[1]);
    };
    assert_eq!(inline.text, "Some bold text.");
    assert_eq!(inline.spans[0].range, 5..9);
    assert_eq!(
        inline.spans[0].style,
        Style {
            bold: true,
            ..Style::default()
        }
    );
    assert_eq!(blocks[2], Block::Rule);
    assert_eq!(blocks[3], Block::Code("fn main() {}".into()));
}

#[test]
fn an_image_stands_between_its_text() {
    let blocks = parse("Before ![shot](https://uploads.linear.app/a/b.png?sig=1) after");
    assert_eq!(
        blocks,
        [
            paragraph("Before "),
            Block::Image {
                url: "https://uploads.linear.app/a/b.png?sig=1".into(),
                alt: "shot".into(),
            },
            paragraph(" after"),
        ]
    );
    assert_eq!(
        image_urls(&blocks),
        ["https://uploads.linear.app/a/b.png?sig=1"]
    );
}

#[test]
fn an_html_image_keeps_only_its_address() {
    let blocks = parse(r#"<img width="300" src="https://github.com/user-attachments/x.png" />"#);
    assert_eq!(
        blocks,
        [Block::Image {
            url: "https://github.com/user-attachments/x.png".into(),
            alt: String::new(),
        }]
    );
}

#[test]
fn lists_nest_and_tasks_keep_their_state() {
    let blocks = parse("- [x] done\n- [ ] open\n  1. first\n  2. second\n");
    let Block::List { start: None, items } = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(items[0].task, Some(true));
    assert_eq!(items[1].task, Some(false));
    assert!(matches!(
        items[1].blocks.last(),
        Some(Block::List { start: Some(1), items }) if items.len() == 2
    ));
    let _: &ListItem = &items[0];
}

#[test]
fn only_web_and_mail_links_are_kept() {
    let blocks = parse("[a](https://a.test) [b](javascript:alert(1)) [c](mailto:x@y.z)");
    let Block::Paragraph(inline) = &blocks[0] else {
        panic!("{blocks:?}");
    };
    let links: Vec<Option<&str>> = inline
        .spans
        .iter()
        .map(|span| span.link.as_deref())
        .collect();
    assert_eq!(links, [Some("https://a.test"), Some("mailto:x@y.z")]);
}

#[test]
fn tables_keep_their_head_and_rows() {
    let blocks = parse("| a | b |\n|---|---|\n| 1 | 2 |\n");
    let Block::Table { head, rows } = &blocks[0] else {
        panic!("{blocks:?}");
    };
    assert_eq!(
        head.iter()
            .map(|cell| cell.text.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(rows[0][1].text, "2");
}

#[test]
fn relative_attachments_resolve_against_the_server() {
    let mut blocks = parse("![a](/api/attachments/12/shot.png)");
    resolve_images(
        &mut blocks,
        &url::Url::parse("http://127.0.0.1:4319/").unwrap(),
    );
    assert_eq!(
        image_urls(&blocks),
        ["http://127.0.0.1:4319/api/attachments/12/shot.png"]
    );
}

#[test]
fn a_huge_body_is_bounded() {
    let body = "line\n\n".repeat(20_000);
    assert!(parse(&body).len() <= 4096);
}
