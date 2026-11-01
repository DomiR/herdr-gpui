//! The timeline is ScrawlKit's: what `scrawl import` reads beside the video.
use super::*;
use crate::browser::annotate::{Marked, Stroke};
use std::time::{Duration, Instant};

fn marked(component: Option<&str>, source: Option<&str>) -> Marked {
    Marked {
        tag: "button".into(),
        selector: "body > button:nth-of-type(2)".into(),
        text: "Buy".into(),
        component: component.map(Into::into),
        source: source.map(Into::into),
    }
}

#[test]
fn strokes_land_on_the_videos_clock_with_their_elements_as_events() {
    let start = Instant::now();
    let mut session = Session::new(start, "http://localhost:3000/pricing", Some("Pricing"));
    session.stroke(
        &Stroke {
            duration_ms: 500,
            points: vec![[10., 20., 0.], [30., 40., 500.]],
            viewport: (1200., 800.),
            elements: vec![marked(
                Some("PricingCard"),
                Some("src/components/PricingCard.tsx:42"),
            )],
        },
        start + Duration::from_secs(3),
    );
    session.navigated(
        start + Duration::from_secs(4),
        "http://localhost:3000/checkout",
        None,
    );
    // The same page again is not a new screen.
    session.navigated(
        start + Duration::from_secs(5),
        "http://localhost:3000/checkout",
        None,
    );
    let json = serde_json::to_value(session.finish((640., 480.))).unwrap();
    assert_eq!(json["app"], "Herdr");
    assert_eq!(json["bundleId"], "so.pen.herdr-gpui");
    assert_eq!(json["device"], "Herdr UI");
    assert_eq!(
        json["screen"],
        serde_json::json!({"width": 1200., "height": 800., "scale": 1.})
    );
    assert_eq!(
        json["strokes"],
        serde_json::json!([{"t0": 2.5, "t1": 3., "points": [[10., 20., 2.5], [30., 40., 3.]]}])
    );
    assert_eq!(
        json["events"],
        serde_json::json!([{
            "t": 3.,
            "label": "marked <button> \u{201c}Buy\u{201d} \u{b7} PricingCard (src/components/PricingCard.tsx:42)"
        }])
    );
    assert_eq!(
        json["screens"],
        serde_json::json!([
            {"t": 0., "name": "Pricing \u{b7} http://localhost:3000/pricing"},
            {"t": 4., "name": "http://localhost:3000/checkout"},
        ])
    );
    assert_eq!(json["context"]["url"], "http://localhost:3000/pricing");
    assert!(json["recordedAt"].as_str().is_some_and(|at| !at.is_empty()));
}

#[test]
fn a_recording_without_strokes_spans_the_page() {
    let session = Session::new(Instant::now(), "http://localhost:3000/", None);
    let json = serde_json::to_value(session.finish((640., 480.))).unwrap();
    assert_eq!(json["screen"]["width"], 640.);
    assert_eq!(json["strokes"], serde_json::json!([]));
}

#[test]
fn a_stroke_that_began_before_the_video_starts_with_it() {
    let start = Instant::now();
    let mut session = Session::new(start, "http://localhost/", None);
    session.stroke(
        &Stroke {
            duration_ms: 2000,
            points: vec![[1., 1., 0.]],
            viewport: (100., 100.),
            elements: Vec::new(),
        },
        start + Duration::from_millis(500),
    );
    let json = serde_json::to_value(session.finish((100., 100.))).unwrap();
    assert_eq!(json["strokes"][0]["t0"], 0.);
    assert_eq!(json["events"], serde_json::json!([]));
}

#[test]
fn marks_name_what_drew_them_else_where_they_are() {
    assert_eq!(timeline::marked_label(&[]), None);
    assert_eq!(
        timeline::marked_label(&[marked(None, None), marked(Some("Card"), None)]).unwrap(),
        "marked <button> \u{201c}Buy\u{201d} at body > button:nth-of-type(2); <button> \u{201c}Buy\u{201d} \u{b7} Card"
    );
    assert_eq!(
        timeline::marked_label(&[marked(None, Some("src/a.tsx:3"))]).unwrap(),
        "marked <button> \u{201c}Buy\u{201d} \u{b7} src/a.tsx:3"
    );
}
