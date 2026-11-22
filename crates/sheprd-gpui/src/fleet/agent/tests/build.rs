//! Which agent build a plain host gets, by its `uname -sm`.

use super::*;
use crate::fleet::agent::build::{self, BUILDS, Error, Located, Source, build_for, choose};
use std::path::{Path, PathBuf};

#[test]
fn each_supported_platform_maps_to_a_release_target() {
    for (uname, target) in [
        ("Linux x86_64\n", "x86_64-unknown-linux-musl"),
        ("Linux amd64", "x86_64-unknown-linux-musl"),
        ("Linux aarch64\n", "aarch64-unknown-linux-musl"),
        ("Linux arm64", "aarch64-unknown-linux-musl"),
        ("Darwin arm64\n", "aarch64-apple-darwin"),
        ("Darwin x86_64", "x86_64-apple-darwin"),
    ] {
        let platform = build::platform(uname).unwrap();
        assert_eq!(build::target(&platform), Some(target), "{uname}");
        assert!(sheprd_updater::AGENT_TARGETS.contains(&target));
    }
}

#[test]
fn every_release_target_is_reachable_from_some_host() {
    for target in sheprd_updater::AGENT_TARGETS {
        let found = [
            "Linux x86_64",
            "Linux aarch64",
            "Darwin x86_64",
            "Darwin arm64",
        ]
        .into_iter()
        .filter_map(|uname| build::platform(uname).ok())
        .any(|platform| build::target(&platform) == Some(target));
        assert!(found, "{target}");
    }
}

#[test]
fn an_unsupported_platform_is_refused_by_name_before_anything_is_copied() {
    for uname in [
        "FreeBSD amd64",
        "OpenBSD arm64",
        "MINGW64_NT-10.0-26100 x86_64",
        "CYGWIN_NT-10.0 x86_64",
        "Linux armv7l",
        "Linux i686",
        "Linux riscv64",
        "Darwin powerpc",
        "SunOS i86pc",
        "",
        "Linux",
    ] {
        match build::platform(uname) {
            Err(Error::Unsupported { platform }) => {
                assert!(platform.len() <= 80, "{platform}");
            }
            other => panic!("{uname:?}: {other:?}"),
        }
    }
}

#[test]
fn what_the_host_prints_is_bounded_and_stripped_of_controls() {
    let hostile = format!("\u{1b}]52;c;AAAA\u{7}Evil {}", "x".repeat(10_000));
    let Err(Error::Unsupported { platform }) = build::platform(&hostile) else {
        panic!("refused");
    };
    assert!(platform.chars().count() <= 80, "{platform}");
    assert!(!platform.chars().any(char::is_control), "{platform:?}");
}

#[test]
fn a_release_sends_only_its_signed_build_whatever_sits_beside_it() {
    let dir = tempfile::tempdir().unwrap();
    let own = dir.path().join("own-agent");
    std::fs::write(&own, b"own").unwrap();
    let local = Platform::local();
    let beside = dir.path().join(BUILDS).join(local.triple()).join(PROGRAM);
    std::fs::create_dir_all(beside.parent().unwrap()).unwrap();
    std::fs::write(&beside, b"beside").unwrap();
    for platform in [
        platform("linux", "x86_64"),
        platform("linux", "aarch64"),
        platform("macos", "x86_64"),
        platform("macos", "aarch64"),
    ] {
        let target = build::target(&platform).unwrap();
        assert_eq!(
            choose(&platform, true, Some(own.clone()), dir.path()).unwrap(),
            Source::Release(target)
        );
    }
}

#[test]
fn a_release_refuses_a_platform_it_ships_no_agent_for() {
    let windows = platform("windows", "x86_64");
    assert!(matches!(
        choose(&windows, true, None, Path::new("/nowhere")),
        Err(Error::Unsupported { .. })
    ));
    assert!(matches!(
        choose(&windows, false, None, Path::new("/nowhere")),
        Err(Error::Unsupported { .. })
    ));
}

#[test]
fn a_development_build_sends_its_own_agent_to_its_kind_and_shipped_builds_to_others() {
    let dir = tempfile::tempdir().unwrap();
    let own = dir.path().join("own-agent");
    std::fs::write(&own, b"own").unwrap();
    let local = Platform::local();
    if build::target(&local).is_some() {
        assert_eq!(
            choose(&local, false, Some(own.clone()), dir.path()).unwrap(),
            Source::Local(own.clone())
        );
    }
    assert_eq!(
        build_for(&local, dir.path(), Some(own.clone())),
        Some(own.clone())
    );
    let other = if local.os == "linux" {
        platform("macos", "aarch64")
    } else {
        platform("linux", "x86_64")
    };
    assert_eq!(build_for(&other, dir.path(), Some(own.clone())), None);
    assert!(matches!(
        choose(&other, false, Some(own.clone()), dir.path()),
        Err(Error::NoBuild { .. })
    ));
    let shipped = dir.path().join(BUILDS).join(other.triple()).join(PROGRAM);
    std::fs::create_dir_all(shipped.parent().unwrap()).unwrap();
    std::fs::write(&shipped, b"shipped").unwrap();
    assert_eq!(
        choose(&other, false, Some(own), dir.path()).unwrap(),
        Source::Local(shipped)
    );
}

#[test]
fn a_located_build_names_its_file_and_a_checked_one_leaves_nothing_behind() {
    let local = Located::Local(PathBuf::from("/beside/sheprd-relay-agent"));
    assert_eq!(local.path(), Path::new("/beside/sheprd-relay-agent"));
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().to_owned();
    let path = folder.join(PROGRAM);
    std::fs::write(&path, b"checked").unwrap();
    let checked = Located::Checked {
        _dir: dir,
        path: path.clone(),
    };
    assert_eq!(checked.path(), path);
    drop(checked);
    assert!(!folder.exists());
}
