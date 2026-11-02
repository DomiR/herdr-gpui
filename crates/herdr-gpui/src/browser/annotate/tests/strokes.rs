//! Pen strokes the page posts while it is recorded, and a recording's note.
use super::*;

fn stroke(body: &str) -> Option<Report> {
    Report::parse(body)
}

#[test]
fn a_stroke_is_parsed_with_its_elements_cleaned() {
    let parsed = stroke(concat!(
        r#"{"kind":"stroke","duration":400,"points":[[10,20,0],[30,40,200],[50,60,400]],"#,
        r#""viewport":{"width":1200,"height":800},"elements":[{"tag":"BUTTON","#,
        r#""selector":"body > button:nth-of-type(1)","text":"Buy\nnow\u001b[201~","#,
        r#""component":"PricingCard","source":"src/components/PricingCard.tsx:42"},"#,
        r##"{"tag":"div","selector":"#plans","text":"","component":"","source":null}]}"##
    ))
    .unwrap();
    assert_eq!(
        parsed,
        Report::Stroke(Stroke {
            duration_ms: 400,
            points: vec![[10., 20., 0.], [30., 40., 200.], [50., 60., 400.]],
            viewport: (1200., 800.),
            elements: vec![
                Marked {
                    tag: "button".into(),
                    selector: "body > button:nth-of-type(1)".into(),
                    text: "Buy now [201~".into(),
                    component: Some("PricingCard".into()),
                    source: Some("src/components/PricingCard.tsx:42".into()),
                },
                Marked {
                    tag: "div".into(),
                    selector: "#plans".into(),
                    text: String::new(),
                    component: None,
                    source: None,
                },
            ],
        })
    );
}

#[test]
fn strokes_out_of_bounds_are_dropped() {
    let body = |duration: &str, points: &str, viewport: &str| {
        format!(
            r#"{{"kind":"stroke","duration":{duration},"points":{points},"viewport":{viewport}}}"#
        )
    };
    let view = r#"{"width":800,"height":600}"#;
    // Points out of the stroke's own time, none, or not a real page.
    for (duration, points, viewport) in [
        ("100", "[[1,1,200]]", view),
        ("100", "[[1,1,-1]]", view),
        ("100", "[]", view),
        ("100", "[[1e12,1,0]]", view),
        ("100", "[[1,1,0]]", r#"{"width":0,"height":600}"#),
        ("99999999", "[[1,1,0]]", view),
    ] {
        assert_eq!(stroke(&body(duration, points, viewport)), None, "{points}");
    }
    let many: Vec<String> = (0..=MAX_STROKE_POINTS)
        .map(|_| "[1,1,0]".to_owned())
        .collect();
    assert_eq!(
        stroke(&body("10", &format!("[{}]", many.join(",")), view)),
        None
    );
    // Only the first few elements are kept.
    let elements: Vec<String> = (0..10)
        .map(|index| format!(r#"{{"tag":"p","selector":"p:nth-of-type({index})"}}"#))
        .collect();
    let Some(Report::Stroke(kept)) = stroke(&format!(
        r#"{{"kind":"stroke","duration":0,"points":[[1,1,0]],"viewport":{view},"elements":[{}]}}"#,
        elements.join(",")
    )) else {
        panic!("a stroke");
    };
    assert_eq!(kept.elements.len(), MAX_MARKED);
}

#[test]
fn the_pen_is_a_page_mode() {
    assert_eq!(Mode::default(), Mode::Pick);
    assert_eq!(Mode::Pen.name(), "pen");
    assert_eq!(Mode::Region.name(), "region");
}

#[test]
fn a_recording_note_carries_scrawls_summary_into_the_prompt() {
    let summary = "[scrawl 0:12 \u{b7} Herdr]\n- #1 (0:03) \u{201c}This button\u{201d}\u{1b}[31m\n  \u{b7} 0:03 marked <button> \u{201c}Buy\u{201d} \u{b7} PricingCard (src/PricingCard.tsx:42)\n";
    let note = Note::recording("/tmp/review.scrawl".into(), summary, 72);
    // The panel's line, where there is a panel.
    #[cfg(any(target_os = "macos", windows))]
    // Fluent isolates the time it puts in the label.
    assert_eq!(
        note.anchor.summary().replace(['\u{2068}', '\u{2069}'], ""),
        "Recording 1:12"
    );
    let Anchor::Recording { summary: kept, .. } = &note.anchor else {
        panic!("a recording");
    };
    // Lines stay; what could steer a terminal does not.
    assert_eq!(kept.lines().count(), 3);
    assert!(!kept.contains('\u{1b}'));
    let text = prompt(&tab(), &[note], &[], "herdr-gpui browser reload");
    assert!(text.contains("1. A 1:12 recording of the page"), "{text}");
    assert!(
        text.contains("built by Scrawl: /tmp/review.scrawl"),
        "{text}"
    );
    assert!(
        text.contains("   - #1 (0:03) \u{201c}This button\u{201d}[31m"),
        "{text}"
    );
    assert!(
        text.contains("PricingCard (src/PricingCard.tsx:42)"),
        "{text}"
    );
    // It quotes page text, so it is fenced as data.
    assert!(
        text.contains("   Scrawl's summary:\n   ```text\n   [scrawl 0:12"),
        "{text}"
    );
    assert!(text.contains("tsx:42)\n   ```\n"), "{text}");
}
