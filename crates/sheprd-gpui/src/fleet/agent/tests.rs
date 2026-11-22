use super::*;
use crate::host_build::Platform;
// For `installed`.
#[cfg(unix)]
use std::path::Path;

mod build;
#[cfg(unix)]
mod installed;
#[cfg(unix)]
mod launchd;
#[cfg(unix)]
mod ssh_port;
mod tokens;

fn public(url: &str) -> Relay {
    Relay {
        url: url.to_owned(),
        token: None,
    }
}

fn platform(os: &'static str, arch: &'static str) -> Platform {
    Platform { os, arch }
}

fn report(keys: &[&str]) -> Report {
    Report::parse(
        serde_json::json!({
            "authorized": 0,
            "identity": "identity",
            "relays": {"https://fra1.relay.sheprd.org": {"state": "listening"}},
            "route": "route",
            "running": true,
            "ssh_host_keys": keys,
            "version": "0.1.0",
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap()
}

const ED25519: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPP";
const RSA: &str = "ssh-rsa AAAAB3NzaC1yc2E";

#[test]
fn only_plain_relay_urls_go_into_a_service() {
    assert!(plain_relay("https://fra1.relay.sheprd.org"));
    assert!(plain_relay("http://127.0.0.1:8443"));
    assert!(plain_relay("https://[::1]:443"));
    for relay in [
        "relay.example.org",
        "https://relay.example.org/$(id)",
        "https://relay.example.org/a b",
        "https://relay.example.org/%h",
        "https://relay.example.org/\"",
        "https://relay.example.org/<x>",
    ] {
        assert!(!plain_relay(relay), "{relay}");
    }
}

#[test]
fn the_service_runs_the_agent_on_each_relay_on_both_systems() {
    let script = service_script(&[
        public("https://fra1.relay.sheprd.org"),
        public("https://nyc1.relay.sheprd.org"),
    ]);
    assert!(script.contains(
        "--forward $port --relay https://fra1.relay.sheprd.org --relay https://nyc1.relay.sheprd.org"
    ));
    assert!(
        script.contains("<string>--relay</string><string>https://nyc1.relay.sheprd.org</string>")
    );
    assert!(script.contains("systemctl --user restart sheprd-relay-agent.service"));
    assert!(script.contains("launchctl bootstrap"));
    // An unchanged, loaded job restarts in place; a new one is loaded,
    // retried while the old one is still on its way out.
    assert!(script.contains("launchctl kickstart -k \"$job\""));
    assert!(script.contains("until launchctl bootstrap"));
    assert!(script.contains("sleep 1"));
    assert!(
        script
            .trim_end()
            .ends_with("\"$agent\" status --json --state-dir \"$state\"")
    );
    // The service runs on the state directory the SSH commands use.
    assert!(script.contains("run --state-dir \\\"$state\\\" --forward $port --relay"));
    assert!(script.contains("<string>--state-dir</string><string>$state</string>"));
    assert!(script.contains("state=\"$XDG_STATE_HOME/sheprd-relay-agent\""));
}

#[test]
fn a_status_reads_and_nothing_printed_means_no_agent() {
    assert!(parse_status(b"  \n").unwrap().is_none());
    let report = report(&[ED25519]);
    let printed = serde_json::to_vec_pretty(&report).unwrap();
    assert_eq!(parse_status(&printed).unwrap(), Some(report));
    assert!(matches!(parse_status(b"nope"), Err(Error::Agent(_))));
}

#[test]
fn a_shared_host_carries_only_keys_this_computer_trusts() {
    let report = report(&[ED25519, RSA]);
    let known = vec![ED25519.to_owned()];
    let relays = vec!["https://fra1.relay.sheprd.org".to_owned()];
    let intro = intro(&report, &known, "build\u{7}-01", "deploy", &relays, 7).unwrap();
    assert_eq!(intro.kind, HostKind::Agent);
    assert_eq!(intro.name, "build-01");
    assert_eq!(intro.record.ssh_host_keys, [ED25519]);
    assert_eq!(intro.record.relays, relays);
    assert_eq!(intro.record.route, "route");
    assert_eq!(intro.id().unwrap(), HostId::plain(ED25519));
    assert_eq!(
        host_id(&report.ssh_host_keys, &known),
        Some(HostId::plain(ED25519))
    );
}

#[test]
fn a_host_whose_first_key_this_computer_does_not_trust_is_not_shared() {
    let reported = report(&[RSA, ED25519]);
    let known = vec![ED25519.to_owned()];
    let relays = vec!["https://fra1.relay.sheprd.org".to_owned()];
    assert!(matches!(
        intro(&reported, &known, "build-01", "deploy", &relays, 7),
        Err(Error::HostKey)
    ));
    assert!(matches!(
        intro(&report(&[]), &known, "build-01", "deploy", &relays, 7),
        Err(Error::HostKey)
    ));
    assert_eq!(host_id(&reported.ssh_host_keys, &known), None);
}

#[test]
fn a_share_waits_for_a_running_agent() {
    let mut stopped = report(&[ED25519]);
    stopped.running = false;
    assert!(matches!(running(stopped), Err(Error::NotRunning)));
    assert!(running(report(&[ED25519])).is_ok());
}

/// Every script this module runs on a host parses as POSIX shell.
#[cfg(unix)]
#[test]
fn the_host_scripts_parse() {
    let scripts = [
        service_script(&[public("https://fra1.relay.sheprd.org")]),
        service_script(&[
            public("https://fra1.relay.sheprd.org"),
            tokens::private("https://own.example.org"),
        ]),
        probe_script("1.2.3"),
        RUN.replace("ARGS", "status --json"),
    ];
    for script in scripts {
        #[allow(clippy::disallowed_methods)] // A local `sh -n`, in a test.
        let checked = std::process::Command::new("sh")
            .args(["-n", "-c", &script])
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "{}\n{script}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}
