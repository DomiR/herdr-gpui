use super::*;

fn ssh(target: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    }
}

#[test]
fn every_enabled_machine_is_a_host_once() {
    let (devbox, devbox_dev, gpu, off) = (
        ssh("devbox"),
        ConnectTarget::Ssh {
            target: "devbox".into(),
            session: "dev".into(),
        },
        ssh("gpu"),
        ssh("off"),
    );
    let socket = ConnectTarget::Socket("/tmp/herdr.sock".into());
    let local = ConnectTarget::Local;
    let known = known([
        ("local", "This Mac", &local, true),
        ("ssh:1", "devbox", &devbox, true),
        ("ssh:2", "devbox (dev)", &devbox_dev, true),
        ("ssh:3", "gpu\u{7}box", &gpu, true),
        ("ssh:4", "off", &off, false),
        ("socket", "custom", &socket, true),
    ]);
    assert_eq!(
        known
            .iter()
            .map(|known| (known.host.id.0.as_str(), known.host.name.as_str()))
            .collect::<Vec<_>>(),
        vec![("ssh:1", "devbox"), ("ssh:3", "gpubox")]
    );
    assert_eq!(known[0].target, devbox);
}

#[test]
fn a_hosts_answer_is_read_strictly() {
    let answer = br#"{"device":"d1","computer":"devbox","fingerprint":"AB:CD","origins":["https://100.64.0.7:7443"]}"#;
    let enrolled = parse_enrolled(answer).unwrap();
    assert_eq!(enrolled.device.0, "d1");
    assert_eq!(enrolled.origins, ["https://100.64.0.7:7443"]);
    assert!(matches!(parse_enrolled(b"not json"), Err(Error::Answer(_))));
    let empty = br#"{"device":"","computer":"devbox","fingerprint":"AB","origins":[]}"#;
    assert!(matches!(parse_enrolled(empty), Err(Error::Answer(_))));
}

#[test]
fn a_failure_reads_with_its_causes() {
    let error = Error::Build(host_build::Error::Binary {
        program: PROGRAM,
        path: "/nowhere".into(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "gone"),
    });
    let reason = reason(&error);
    assert!(reason.starts_with("Could not read the herdr-remote-control build"));
    assert!(reason.ends_with(": gone"));
}

#[test]
fn a_host_job_without_a_host_fails_without_running_anything() {
    let job = Job {
        pairing: HostPairing {
            device: DeviceId("d1".into()),
            host: hosts::HostId("ssh:gone".into()),
            name: "iPhone".into(),
            public_key: "KEY".into(),
            confirm: false,
            replace: None,
        },
        target: None,
        via: "MacBook Pro".into(),
        apns: None,
    };
    assert!(matches!(
        pair(&job, &AtomicBool::new(false)),
        HostOutcome::Failed { reason } if reason == Error::Gone.to_string()
    ));
}

/// The probe, copy, and revoke scripts, run for real against a temporary
/// home: the probe sees the copy once it is in place, and revoking on a
/// host without the program does nothing.
#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn the_host_scripts_find_a_copy_and_revoke_quietly_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let build = dir.path().join("build");
    std::fs::write(&build, b"#!/bin/sh\necho \"$@\" > \"$HOME/args\"\n").unwrap();
    let (_, sha256) = host_build::digest(PROGRAM, &build).unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let mut host = Host::new(&ConnectTarget::Local).unwrap();
    host.env = vec![
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let device = DeviceId("d'1".into());
    // Nothing installed: nothing to revoke, and the probe has no sum.
    host.capture(&revoke_script(&device), &cancelled).unwrap();
    let probe = host.capture(&probe_script("9.9.9"), &cancelled).unwrap();
    let probe = String::from_utf8(probe).unwrap();
    let mut lines = probe.lines();
    assert!(Platform::parse(lines.next().unwrap()).is_ok());
    assert_eq!(lines.next(), None);
    let file = std::fs::File::open(&build).unwrap();
    host.stream(
        &host_build::copy_script(PROGRAM, "9.9.9", &sha256),
        file,
        Vec::new(),
        ScriptLimits {
            output: 1024,
            idle: Duration::from_secs(10),
        },
        &cancelled,
    )
    .unwrap();
    let probe = host.capture(&probe_script("9.9.9"), &cancelled).unwrap();
    assert_eq!(
        String::from_utf8(probe).unwrap().lines().nth(1),
        Some(sha256.as_str())
    );
    // With the copy on `PATH`, revoking runs it with the quoted device.
    host.capture(&revoke_script(&device), &cancelled).unwrap();
    assert_eq!(
        std::fs::read_to_string(home.join("args")).unwrap(),
        "revoke --device d'1\n"
    );
}
