//! Recording a page review from its notes panel (`browser::record`). Record
//! starts the video of the page with the microphone and turns on the pen;
//! each stroke and navigation goes on the timeline. Stop finishes the video,
//! has Scrawl build the bundle off the UI thread, and queues it as a note
//! the user sends with the others. One page records at a time.

use super::super::{
    TabId,
    annotate::{self, MAX_NOTES, Note, Stroke},
    record,
};
use crate::{HerdrWindow, window::Flash};
use gpui::{prelude::*, *};
use std::{
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

/// How often the panel's clock redraws while recording, and how often a
/// thread's answer is looked for.
const TICK: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(100);

/// The window's recording, from asking macOS to the note it becomes.
pub(crate) enum Recording {
    /// macOS is setting the stream up, or asking for permission.
    Starting {
        tab: TabId,
    },
    Live(Box<Live>),
    /// The video is being finished and built into a bundle.
    Building {
        tab: TabId,
    },
}

/// A recording under way: its video, and the timeline beside it.
pub(crate) struct Live {
    pub tab: TabId,
    session: record::Session,
    pub capture: record::Capture,
    /// The page's size in points, for a timeline without strokes.
    page: (f64, f64),
}

impl Recording {
    pub(crate) fn tab(&self) -> TabId {
        match self {
            Self::Starting { tab } | Self::Building { tab } => *tab,
            Self::Live(live) => live.tab,
        }
    }

    /// What the panel's Record button says for this recording.
    pub(crate) fn label(&self, now: Instant) -> String {
        match self {
            Self::Starting { .. } => "Starting\u{2026}".into(),
            Self::Live(live) => {
                let seconds = now
                    .saturating_duration_since(live.session.started())
                    .as_secs();
                format!("Stop {}:{:02}", seconds / 60, seconds % 60)
            }
            Self::Building { .. } => "Building\u{2026}".into(),
        }
    }
}

impl HerdrWindow {
    /// Record starts recording tab `id`; on the page being recorded it stops.
    pub(in crate::browser) fn toggle_recording(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match &self.browser.annotations.recording {
            None => self.start_recording(id, window, cx),
            Some(Recording::Live(live)) if live.tab == id => self.stop_recording(cx),
            Some(Recording::Live(_)) => {
                self.show_flash(Flash::warning("Another page is being recorded"), cx);
            }
            Some(_) => {}
        }
    }

    fn start_recording(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let page = self
            .browser
            .pages
            .drawn_at(id)
            .filter(|_| self.browser.pages.is_shown(id));
        let Some(page) = page else {
            self.show_flash(Flash::warning(record::Error::PageHidden.to_string()), cx);
            return;
        };
        let bounds = window.bounds();
        let points = |pixels: Pixels| f64::from(f32::from(pixels));
        let target = record::Target {
            pid: std::process::id() as i32,
            window: (
                points(bounds.origin.x),
                points(bounds.origin.y),
                points(bounds.size.width),
                points(bounds.size.height),
            ),
            page: (
                points(page.origin.x),
                points(page.origin.y),
                points(page.size.width),
                points(page.size.height),
            ),
            scale: f64::from(window.scale_factor()),
        };
        let (url, title) = super::super::view::store(cx)
            .and_then(|store| store.get(id))
            .map(|tab| {
                (
                    tab.location
                        .as_ref()
                        .map(super::super::Location::display)
                        .unwrap_or_default(),
                    tab.title.clone(),
                )
            })
            .unwrap_or_default();
        if !self.browser.annotations.armed(id) {
            self.toggle_annotating(id, window, cx);
        }
        self.browser.annotations.recording = Some(Recording::Starting { tab: id });
        // Scrawl is looked for first, so a missing one says so before macOS
        // asks for anything.
        let starting = on_thread(move || {
            let home = herdr_client::local_path::home_dir();
            record::find_scrawl(home.as_deref()).ok_or(record::Error::ScrawlMissing)?;
            let video = record::work_dir()?.join(format!("page-{}.mp4", stamp()));
            record::Capture::start(target, &video)
        });
        cx.spawn(async move |this, cx| {
            let started = answer(starting, cx).await;
            this.update(cx, |this, cx| {
                // The page may have closed, or its notes been put away, while
                // macOS set up; then this recording is no one's.
                let ours = matches!(
                    this.browser.annotations.recording,
                    Some(Recording::Starting { tab }) if tab == id
                );
                if ours {
                    this.browser.annotations.recording = None;
                }
                let still = ours && this.browser.annotations.armed(id);
                match started {
                    Ok(capture) if still => {
                        this.browser.annotations.recording =
                            Some(Recording::Live(Box::new(Live {
                                tab: id,
                                session: record::Session::new(capture.began(), &url, Some(&title)),
                                capture,
                                page: (target.page.2, target.page.3),
                            })));
                        this.set_pen(id, true, cx);
                        this.tick_recording(cx);
                        this.show_flash(
                            Flash::success("Recording: talk, and draw on the page with the pen"),
                            cx,
                        );
                    }
                    Ok(capture) => discard(capture),
                    Err(error) if still => {
                        tracing::warn!(%error, "Could not start recording the page");
                        this.show_flash(Flash::warning(error.to_string()), cx);
                    }
                    Err(error) => {
                        tracing::debug!(%error, "A recording no one waits for did not start");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        let Some(Recording::Live(live)) = self.browser.annotations.recording.take() else {
            return;
        };
        let Live {
            tab,
            session,
            capture,
            page,
        } = *live;
        self.browser.annotations.recording = Some(Recording::Building { tab });
        self.set_pen(tab, false, cx);
        let seconds = Instant::now()
            .saturating_duration_since(session.started())
            .as_secs();
        let building = on_thread(move || {
            let video = capture.finish()?;
            let timeline = video.with_extension("json");
            std::fs::write(&timeline, serde_json::to_vec(&session.finish(page))?)?;
            let home = herdr_client::local_path::home_dir().ok_or(crate::Error::MissingHome)?;
            let scrawl = record::find_scrawl(Some(&home)).ok_or(record::Error::ScrawlMissing)?;
            let out = record::bundle_dir(&home, SystemTime::now());
            let bundle = record::import(&scrawl, &video, &timeline, &out)?;
            // Scrawl copied both into the bundle.
            let _ = std::fs::remove_file(&video);
            let _ = std::fs::remove_file(&timeline);
            Ok(bundle)
        });
        cx.spawn(async move |this, cx| {
            let built = answer(building, cx).await;
            this.update(cx, |this, cx| {
                // Only this build's own state ends here: a page closed while
                // it built may already have made way for another recording.
                if matches!(
                    this.browser.annotations.recording,
                    Some(Recording::Building { tab: building }) if building == tab
                ) {
                    this.browser.annotations.recording = None;
                }
                match built {
                    Ok(bundle) => this.add_recording(tab, bundle, seconds, cx),
                    Err(error) => {
                        tracing::warn!(%error, "Could not build the page recording");
                        this.show_flash(Flash::warning(error.to_string()), cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn add_recording(
        &mut self,
        id: TabId,
        bundle: record::Bundle,
        seconds: u64,
        cx: &mut Context<Self>,
    ) {
        // The page closed while Scrawl built: say where the review is.
        if !self.browser.annotations.tabs.contains_key(&id) {
            self.show_flash(
                Flash::warning(format!(
                    "The page closed; its recording is in {}",
                    bundle.dir.display()
                )),
                cx,
            );
            return;
        }
        let notes = self.tab_notes(id);
        if notes.notes.len() >= MAX_NOTES {
            self.show_flash(
                Flash::warning(format!(
                    "No room for the recording's note; it is in {}",
                    bundle.dir.display()
                )),
                cx,
            );
            return;
        }
        notes
            .notes
            .push(Note::recording(bundle.dir, &bundle.summary, seconds));
        self.show_flash(
            Flash::success("The recording is in the notes; send it to the agent"),
            cx,
        );
    }

    /// Turns the pen on or off in tab `id`'s page.
    pub(in crate::browser) fn set_pen(&mut self, id: TabId, on: bool, cx: &mut Context<Self>) {
        let notes = self.tab_notes(id);
        notes.pen = on;
        let mode = notes.mode();
        self.browser.pages.script(id, &annotate::mode_script(mode));
        cx.notify();
    }

    /// A stroke the page posted, for the recording under way.
    pub(in crate::browser) fn page_stroke(&mut self, id: TabId, stroke: &Stroke) {
        if let Some(Recording::Live(live)) = &mut self.browser.annotations.recording
            && live.tab == id
        {
            live.session.stroke(stroke, Instant::now());
        }
    }

    /// Tab `id`'s page loaded `url`: a new screen on the recording's timeline.
    pub(crate) fn recording_navigated(&mut self, id: TabId, url: &str, cx: &App) {
        if let Some(Recording::Live(live)) = &mut self.browser.annotations.recording
            && live.tab == id
        {
            let title = super::super::view::store(cx)
                .and_then(|store| store.get(id))
                .map(|tab| tab.title.clone());
            live.session
                .navigated(Instant::now(), url, title.as_deref());
        }
    }

    /// Redraws the panel's clock while the recording runs.
    fn tick_recording(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let live = this
                    .update(cx, |this, cx| {
                        let live =
                            matches!(this.browser.annotations.recording, Some(Recording::Live(_)));
                        if live {
                            cx.notify();
                        }
                        live
                    })
                    .unwrap_or(false);
                if !live {
                    break;
                }
            }
        })
        .detach();
    }
}

/// Stops a recording no one wants and leaves its video to the state folder.
pub(super) fn discard(capture: record::Capture) {
    drop(on_thread(move || capture.finish()));
}

/// Runs `work` on a thread of its own and hands back its answer. Starting can
/// wait on macOS's permission prompts and building on Scrawl for minutes,
/// which would hold one of GPUI's few background threads.
fn on_thread<T: Send + 'static>(
    work: impl FnOnce() -> crate::Result<T> + Send + 'static,
) -> mpsc::Receiver<crate::Result<T>> {
    let (sender, receiver) = mpsc::channel();
    let failed = sender.clone();
    let spawned = std::thread::Builder::new()
        .name("herdr-page-record".into())
        .spawn(move || {
            let _ = sender.send(work());
        });
    if let Err(error) = spawned {
        let _ = failed.send(Err(error.into()));
    }
    receiver
}

/// Waits for `on_thread`'s answer without blocking the UI thread.
async fn answer<T>(receiver: mpsc::Receiver<crate::Result<T>>, cx: &AsyncApp) -> crate::Result<T> {
    loop {
        match receiver.try_recv() {
            Ok(answer) => return answer,
            Err(mpsc::TryRecvError::Empty) => cx.background_executor().timer(POLL).await,
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(
                    record::Error::RecordingFailed("the recording thread stopped".into()).into(),
                );
            }
        }
    }
}

fn stamp() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
