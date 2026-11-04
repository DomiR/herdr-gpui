//! Finding Scrawl and having it build the bundle.
use super::*;
use std::{path::Path, time::SystemTime};

#[cfg(unix)]
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn scrawl_is_found_in_the_users_applications_when_executable() {
    let home = tempfile::tempdir().unwrap();
    let binary = home
        .path()
        .join("Applications/Scrawl.app/Contents/MacOS/scrawl");
    // /Applications may hold the real one; only the home copy is in our hands.
    let system = find_scrawl(None);
    assert_eq!(find_scrawl(Some(home.path())), system);
    script(&binary, "exit 0");
    let found = find_scrawl(Some(home.path()));
    assert!(
        found == system.clone().or(Some(binary.clone())),
        "{found:?}"
    );
    if system.is_none() {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(find_scrawl(Some(home.path())), None);
    }
}

#[test]
fn bundles_go_in_scrawls_captures_named_after_the_moment() {
    let at = SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_793_000_000_123);
    assert_eq!(
        bundle_dir(Path::new("/Users/me"), at),
        Path::new(
            "/Users/me/Library/Application Support/scrawl/captures/scrawl-1793000000123-herdr.scrawl"
        )
    );
    let args = scrawl::import_args(Path::new("/v.mp4"), Path::new("/t.json"), Path::new("/b"));
    assert_eq!(
        args,
        ["import", "/v.mp4", "--timeline", "/t.json", "--out", "/b"]
    );
}

#[cfg(unix)]
#[test]
fn import_reads_the_summary_scrawl_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let scrawl = dir.path().join("scrawl");
    // Writes the summary into --out, as Scrawl does.
    script(
        &scrawl,
        r#"out=""; while [ $# -gt 0 ]; do [ "$1" = "--out" ] && out="$2"; shift; done
mkdir -p "$out" && printf '[scrawl 0:05]\n- #1 (0:01) "here"\n' > "$out/summary.md""#,
    );
    let video = dir.path().join("page.mp4");
    std::fs::write(&video, b"").unwrap();
    let out = dir.path().join("review.scrawl");
    let bundle = import(&scrawl, &video, &dir.path().join("page.json"), &out).unwrap();
    assert_eq!(bundle.dir, out);
    assert_eq!(bundle.summary, "[scrawl 0:05]\n- #1 (0:01) \"here\"\n");
    // Its log is gone once it worked.
    assert!(!video.with_extension("log").exists());
}

#[cfg(unix)]
#[test]
fn a_failed_import_says_scrawls_last_word() {
    let dir = tempfile::tempdir().unwrap();
    let scrawl = dir.path().join("scrawl");
    script(
        &scrawl,
        "echo building >&2; echo 'Error: The recording is empty' >&2; exit 1",
    );
    let video = dir.path().join("page.mp4");
    std::fs::write(&video, b"").unwrap();
    let error = import(
        &scrawl,
        &video,
        &dir.path().join("page.json"),
        &dir.path().join("b"),
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            crate::Error::Record(Error::Import { detail }) if detail == "Error: The recording is empty"
        ),
        "{error:?}"
    );
    // One that exits cleanly without a summary is not a review either.
    script(&scrawl, "exit 0");
    let error = import(
        &scrawl,
        &video,
        &dir.path().join("page.json"),
        &dir.path().join("c"),
    )
    .unwrap_err();
    assert!(
        matches!(&error, crate::Error::Record(Error::MissingSummary { .. })),
        "{error:?}"
    );
}

#[test]
fn scrawls_last_word_is_one_bounded_line() {
    assert_eq!(scrawl::last_line("a\nb\n\n  \n"), "b");
    assert_eq!(scrawl::last_line(""), "it exited without saying why");
    assert!(scrawl::last_line(&"x".repeat(5000)).chars().count() <= 401);
}
