//! A page recording's timeline in ScrawlKit's format, the `.json` `scrawl
//! import` reads beside the video: times in seconds from the video's start,
//! points in the page's CSS pixels, which `screen` says the video spans.
//! Each pen stroke goes in as one, and the elements under it as an app event
//! at its end, so Scrawl writes them beside the words said as it was drawn.

use crate::browser::annotate::{Marked, Stroke};
use serde::Serialize;
use std::{collections::BTreeMap, time::Instant};

/// The most strokes and events one recording keeps; more are dropped.
const MAX_STROKES: usize = 500;
const MAX_EVENTS: usize = 1000;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Timeline {
    app: &'static str,
    bundle_id: &'static str,
    /// What Scrawl names as the recorded device, in place of an iPhone.
    device: &'static str,
    recorded_at: String,
    short_version: &'static str,
    screen: Screen,
    events: Vec<Event>,
    strokes: Vec<TimelineStroke>,
    context: BTreeMap<String, String>,
    screens: Vec<ScreenName>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct Screen {
    width: f64,
    height: f64,
    scale: f64,
}

#[derive(Debug, PartialEq, Serialize)]
struct Event {
    t: f64,
    label: String,
}

#[derive(Debug, PartialEq, Serialize)]
struct TimelineStroke {
    t0: f64,
    t1: f64,
    points: Vec<[f64; 3]>,
}

#[derive(Debug, PartialEq, Serialize)]
struct ScreenName {
    t: f64,
    name: String,
}

/// A recording under way: when its video started, and what happened since.
#[derive(Debug)]
pub(crate) struct Session {
    started: Instant,
    recorded_at: chrono::DateTime<chrono::Utc>,
    screen: Option<Screen>,
    events: Vec<Event>,
    strokes: Vec<TimelineStroke>,
    context: BTreeMap<String, String>,
    screens: Vec<ScreenName>,
}

impl Session {
    /// A session whose video started at `started`, of the page at `url`.
    pub(crate) fn new(started: Instant, url: &str, title: Option<&str>) -> Self {
        let mut context = BTreeMap::new();
        context.insert("url".to_owned(), url.to_owned());
        let mut session = Self {
            started,
            recorded_at: chrono::Utc::now(),
            screen: None,
            events: Vec::new(),
            strokes: Vec::new(),
            context,
            screens: Vec::new(),
        };
        session.navigated(started, url, title);
        session
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn started(&self) -> Instant {
        self.started
    }

    fn at(&self, now: Instant) -> f64 {
        now.saturating_duration_since(self.started).as_secs_f64()
    }

    /// A stroke the page reported as finished at `now`. Its points carry
    /// times from the stroke's start, which the page measured; the end is
    /// the app's, so both clocks need not agree.
    pub(crate) fn stroke(&mut self, stroke: &Stroke, now: Instant) {
        if self.strokes.len() >= MAX_STROKES {
            return;
        }
        let t1 = self.at(now);
        let t0 = (t1 - stroke.duration_ms as f64 / 1000.).max(0.);
        let points = stroke
            .points
            .iter()
            .map(|&[x, y, dt]| [x, y, t0 + dt / 1000.])
            .collect();
        // The video is the page, so the page's view is the screen; the first
        // stroke's view stands for the recording's.
        self.screen.get_or_insert(Screen {
            width: stroke.viewport.0,
            height: stroke.viewport.1,
            scale: 1.,
        });
        self.strokes.push(TimelineStroke { t0, t1, points });
        if let Some(label) = marked_label(&stroke.elements) {
            self.event(t1, label);
        }
    }

    /// The page moved to `url` at `now`: a new screen for the agent.
    pub(crate) fn navigated(&mut self, now: Instant, url: &str, title: Option<&str>) {
        let t = self.at(now);
        let name = match title.filter(|title| !title.is_empty()) {
            Some(title) => format!("{title} \u{b7} {url}"),
            None => url.to_owned(),
        };
        if self.screens.last().is_some_and(|last| last.name == name) {
            return;
        }
        self.screens.push(ScreenName { t, name });
    }

    fn event(&mut self, t: f64, label: String) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(Event { t, label });
        }
    }

    /// The timeline, for a video whose page was `fallback` points across
    /// when no stroke said otherwise.
    pub(crate) fn finish(self, fallback: (f64, f64)) -> Timeline {
        Timeline {
            // i18n: data
            app: "Herdr",
            bundle_id: "so.pen.herdr-gpui",
            // i18n: data
            device: "Herdr UI",
            recorded_at: self.recorded_at.to_rfc3339(),
            short_version: herdr_updater::APP_VERSION,
            screen: self.screen.unwrap_or(Screen {
                width: fallback.0,
                height: fallback.1,
                scale: 1.,
            }),
            events: self.events,
            strokes: self.strokes,
            context: self.context,
            screens: self.screens,
        }
    }
}

/// What lies under a stroke, as one line for the timeline: each element's
/// tag and text, then the component and source file that drew it when the
/// page says, so the agent goes straight to the file.
pub(crate) fn marked_label(elements: &[Marked]) -> Option<String> {
    if elements.is_empty() {
        return None;
    }
    let described: Vec<String> = elements
        .iter()
        .map(|element| {
            let mut text = format!("<{}>", element.tag);
            if !element.text.is_empty() {
                text.push_str(&format!(" \u{201c}{}\u{201d}", element.text));
            }
            match (&element.component, &element.source) {
                (Some(component), Some(source)) => {
                    text.push_str(&format!(" \u{b7} {component} ({source})"));
                }
                (Some(component), None) => text.push_str(&format!(" \u{b7} {component}")),
                (None, Some(source)) => text.push_str(&format!(" \u{b7} {source}")),
                (None, None) => text.push_str(&format!(" at {}", element.selector)),
            }
            text
        })
        .collect();
    Some(format!("marked {}", described.join("; ")))
}
