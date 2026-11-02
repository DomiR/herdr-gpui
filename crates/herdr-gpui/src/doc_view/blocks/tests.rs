#![allow(clippy::unwrap_used)]
use super::*;
use herdr_doc::markdown;

fn found(text: &str) -> Vec<(BlockKind, Range<usize>, Option<usize>)> {
    let lines = markdown::lines(text);
    blocks(text, &lines)
        .into_iter()
        .map(|(block, _)| (block.kind, block.body, block.close))
        .collect()
}

#[test]
fn fences_diagrams_and_math_are_blocks() {
    let text = "intro\n```rust\nfn a() {}\n```\n```mermaid\ngraph TD; A-->B\n```\n$$\nx^2\n$$\n```\nplain\n```\n";
    assert_eq!(
        found(text),
        vec![
            (BlockKind::Code, 2..3, Some(3)),
            (BlockKind::Diagram, 5..6, Some(6)),
            (BlockKind::Math, 8..9, Some(9)),
        ]
    );
}

#[test]
fn an_unclosed_diagram_stays_code() {
    assert!(found("```mermaid\ngraph TD; A-->B\n").is_empty());
    assert!(found("$$\n$$\n").is_empty());
}
