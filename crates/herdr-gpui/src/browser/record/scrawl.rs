//! Scrawl, which builds the review: `scrawl import VIDEO --timeline JSON
//! --out BUNDLE` transcribes the narration with the transcriber the user set
//! in Scrawl (Apple's or Hex), cuts the keyframes at the strokes and the
//! words, and writes `summary.md` for the agent. The bundle goes in Scrawl's
//! own captures folder, so Scrawl lists it and clears it out with its own.

use super::Error;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant, SystemTime},
};

/// Where Scrawl.app keeps its command, in the places macOS installs apps.
// i18n: data
const APP_BINARY: &str = "Scrawl.app/Contents/MacOS/scrawl";
/// Transcribing a long narration on device takes a while; a build that runs
/// longer than this is stuck.
const IMPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The most of Scrawl's summary a note keeps, and of its error output.
const MAX_SUMMARY_BYTES: u64 = 32 * 1024;
const MAX_DETAIL_CHARS: usize = 400;

/// A review Scrawl built: its folder and its summary for the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bundle {
    pub dir: PathBuf,
    pub summary: String,
}

/// The `scrawl` command inside Scrawl.app, in `/Applications` or the
/// user's own `Applications`. A GUI app's PATH is not the shell's, so PATH
/// is not searched.
pub(crate) fn find(home: Option<&Path>) -> Option<PathBuf> {
    let roots = [
        // i18n: data
        Some(PathBuf::from("/Applications")),
        // i18n: data
        home.map(|home| home.join("Applications")),
    ];
    roots
        .into_iter()
        .flatten()
        .map(|root| root.join(APP_BINARY))
        .find(|path| executable(path))
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    metadata.is_file()
}

/// Where Scrawl keeps its captures, for a bundle named after `now`.
pub(crate) fn bundle_dir(home: &Path, now: SystemTime) -> PathBuf {
    let stamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    // i18n: data
    home.join("Library/Application Support/scrawl/captures")
        .join(format!("scrawl-{stamp}-herdr.scrawl"))
}

/// The arguments that build `out` from `video` and `timeline`.
pub(crate) fn import_args(video: &Path, timeline: &Path, out: &Path) -> Vec<std::ffi::OsString> {
    vec![
        "import".into(),
        video.into(),
        "--timeline".into(),
        timeline.into(),
        "--out".into(),
        out.into(),
    ]
}

/// Runs `scrawl import` and reads the summary it wrote. Its output goes to a
/// log beside the video rather than a pipe, so a chatty build never blocks
/// on a full pipe. Blocking; run off the UI thread.
pub(crate) fn import(
    scrawl: &Path,
    video: &Path,
    timeline: &Path,
    out: &Path,
) -> crate::Result<Bundle> {
    let log_path = video.with_extension("log");
    let log = std::fs::File::create(&log_path)?;
    let mut command = herdr_client::new_command(scrawl);
    command
        .args(import_args(video, timeline, out))
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    let mut child = command.spawn()?;
    let deadline = Instant::now() + IMPORT_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::ImportTimeout(IMPORT_TIMEOUT).into());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if !status.success() {
        let output = std::fs::read_to_string(&log_path).unwrap_or_default();
        return Err(Error::Import {
            detail: last_line(&output),
        }
        .into());
    }
    let _ = std::fs::remove_file(&log_path);
    let summary_path = out.join("summary.md");
    let summary = read_bounded(&summary_path).map_err(|source| Error::MissingSummary {
        path: summary_path,
        source,
    })?;
    Ok(Bundle {
        dir: out.to_owned(),
        summary,
    })
}

/// The last line Scrawl printed that says something, cut short: its error.
pub(crate) fn last_line(output: &str) -> String {
    let line = output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        // i18n: data
        .unwrap_or("it exited without saying why");
    crate::notifications::safe_text(line, MAX_DETAIL_CHARS)
}

fn read_bounded(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_SUMMARY_BYTES)
        .read_to_string(&mut text)?;
    Ok(text)
}
