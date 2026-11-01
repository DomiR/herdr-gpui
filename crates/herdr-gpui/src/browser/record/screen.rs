//! The video: ScreenCaptureKit records the page's part of this app's window,
//! with the microphone, straight to an H.264 `.mp4` (macOS 15's recording
//! output), which is what `scrawl import` reads the narration from. Every
//! call blocks until macOS answers; run them off the UI thread.
//!
//! macOS asks once for Screen Recording and for the microphone. A Screen
//! Recording grant reaches the app only after it is opened again.

use super::Error;
use screencapturekit::{
    prelude::*,
    recording_output::{
        RecordingCallbacks, SCRecordingOutput, SCRecordingOutputCodec,
        SCRecordingOutputConfiguration, SCRecordingOutputFileType,
    },
};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

/// How long the file may take to be finished once the stream stops, and to
/// begin once it starts.
const FINISH_TIMEOUT: Duration = Duration::from_secs(30);
const BEGIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Frames a second: enough to follow a pointer, and small files.
const FPS: u32 = 30;

/// What to record: this process's window at `window` on screen (points,
/// top-left origin), and the page's rectangle inside it, in the window's
/// points, at `scale` pixels a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Target {
    pub pid: i32,
    pub window: (f64, f64, f64, f64),
    pub page: (f64, f64, f64, f64),
    pub scale: f64,
}

/// A recording under way. Dropping it without [`Capture::finish`] stops the
/// stream and leaves whatever was written.
pub(crate) struct Capture {
    stream: SCStream,
    output: SCRecordingOutput,
    finished: mpsc::Receiver<Result<(), Error>>,
    path: PathBuf,
    /// When the file began, which is the timeline's zero.
    began: Instant,
}

impl Capture {
    /// Starts recording `target` to `path`.
    pub(crate) fn start(target: Target, path: &Path) -> crate::Result<Self> {
        if !SCRecordingOutput::is_available() {
            return Err(Error::RecordingUnavailable.into());
        }
        let content = SCShareableContent::create()
            .with_on_screen_windows_only(true)
            .get()
            .map_err(Error::ScreenRecording)?;
        let window = own_window(&content.windows(), &target).ok_or(Error::NoWindow)?;
        let filter = SCContentFilter::create()
            .with_window(&window)
            .build()
            .map_err(Error::Stream)?;
        let (x, y, width, height) = target.page;
        // H.264 wants even sizes.
        let even = |points: f64| ((points * target.scale).round() as u32 / 2 * 2).max(2);
        let config = SCStreamConfiguration::new()
            .with_source_rect(CGRect::new(x, y, width, height))
            .with_width(even(width))
            .with_height(even(height))
            .with_shows_cursor(true)
            .with_fps(FPS)
            .with_captures_microphone(true)
            .map_err(Error::Stream)?;
        let (done, finished) = mpsc::channel();
        let failed = done.clone();
        let (begin, beginning) = mpsc::channel();
        let callbacks = RecordingCallbacks::new()
            .on_start(move || {
                let _ = begin.send(Instant::now());
            })
            .on_finish(move || {
                let _ = done.send(Ok(()));
            })
            .on_fail(move |error| {
                let _ = failed.send(Err(Error::RecordingFailed(error)));
            });
        let recording = SCRecordingOutputConfiguration::new()
            .map_err(|_| Error::RecordingUnavailable)?
            .with_output_url(path)
            .map_err(Error::VideoPath)?
            .with_video_codec(SCRecordingOutputCodec::H264)
            .with_output_file_type(SCRecordingOutputFileType::MP4);
        let output = SCRecordingOutput::new_with_delegate(&recording, callbacks)
            .ok_or(Error::RecordingUnavailable)?;
        let stream = SCStream::new(&filter, &config).map_err(Error::Stream)?;
        stream
            .add_recording_output(&output)
            .map_err(Error::Stream)?;
        stream.start_capture().map_err(Error::Stream)?;
        // Strokes are timed from when the file began; without word from
        // macOS, from now.
        let began = beginning
            .recv_timeout(BEGIN_TIMEOUT)
            .unwrap_or_else(|_| Instant::now());
        Ok(Self {
            stream,
            output,
            finished,
            path: path.to_owned(),
            began,
        })
    }

    /// When the video began.
    pub(crate) fn began(&self) -> Instant {
        self.began
    }

    /// Stops recording and waits for the file to be finished.
    pub(crate) fn finish(self) -> crate::Result<PathBuf> {
        self.stream
            .remove_recording_output(&self.output)
            .map_err(Error::Stream)?;
        self.stream.stop_capture().map_err(Error::Stream)?;
        match self.finished.recv_timeout(FINISH_TIMEOUT) {
            Ok(Ok(())) => Ok(self.path),
            Ok(Err(error)) => Err(error.into()),
            Err(_) => Err(Error::RecordingTimeout.into()),
        }
    }
}

/// This process's on-screen window that best matches `target.window`: the
/// same size, then the nearest origin.
fn own_window(windows: &[SCWindow], target: &Target) -> Option<SCWindow> {
    let (x, y, width, height) = target.window;
    windows
        .iter()
        .filter(|window| {
            window
                .owning_application()
                .is_some_and(|app| app.process_id() == target.pid)
        })
        .map(|window| {
            let frame = window.frame();
            let size = (frame.size.width - width).abs() + (frame.size.height - height).abs();
            let origin = (frame.origin.x - x).abs() + (frame.origin.y - y).abs();
            (size, origin, window)
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)))
        .map(|(_, _, window)| window.clone())
}
