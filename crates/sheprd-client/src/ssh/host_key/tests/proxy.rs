//! Only the outer `ssh` writes the proof: it is read from `ssh`'s own log
//! (`-E`), never the stderr a proxy shares.

use super::*;
use std::os::unix::fs::PermissionsExt as _;

/// A completed exchange as `ssh` logs it.
const LOGGED: &str = "debug1: Server host key: ssh-ed25519 SHA256:abc
debug1: Host 'host' is known and matches the ED25519 host key.
debug1: SSH2_MSG_NEWKEYS sent
debug1: expecting SSH2_MSG_NEWKEYS
debug1: SSH2_MSG_NEWKEYS received
debug1: SSH2_MSG_SERVICE_ACCEPT received
debug1: No more authentication methods to try.
";

/// Writes an executable `sh` script at `path`.
fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A stand-in proxy prints the proof lines on the stderr it shares with
/// `ssh`, then gives up: `ssh` exchanged no key, and its own log says so.
#[test]
fn a_proxy_printing_the_proof_lines_confirms_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let log = scratch.path().join("ssh.log");
    let proxy = scratch.path().join("proxy");
    script(&proxy, &format!("cat >&2 <<'EOF'\n{LOGGED}EOF\nexit 1\n"));
    let mut command = crate::environment::command("ssh");
    command
        .arg("-E")
        .arg(&log)
        .args(["-F", "/dev/null", "-o", "LogLevel=DEBUG1"])
        .args(["-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes"])
        .args(["-o", "UserKnownHostsFile=/dev/null", "-o"])
        .arg(format!("ProxyCommand={}", proxy.display()))
        .args(["--", "proxied.invalid", "true"]);
    assert!(!proven(&mut command, &log).unwrap());
    // What the proxy printed went to stderr, none of it to the log.
    let logged = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(!logged.contains("SSH2_MSG_NEWKEYS received"), "{logged}");
}

/// A completed exchange, as `ssh` logs it through `-E`, confirms; the same
/// log with a failed host key check does not, nor does no log at all.
#[test]
fn a_completed_exchange_logged_through_the_log_file_confirms() {
    let scratch = tempfile::tempdir().unwrap();
    let log = scratch.path().join("ssh.log");
    // A stand-in `ssh` that writes `logged` to its `-E` file.
    let fake = scratch.path().join("ssh");
    let logging = |logged: Option<&str>| {
        let write = logged.map_or_else(String::new, |logged| {
            format!("cat > \"$2\" <<'EOF'\n{logged}EOF\n")
        });
        script(
            &fake,
            &format!("while [ \"$1\" != -E ]; do shift; done\n{write}exit 255\n"),
        );
        let _ = std::fs::remove_file(&log);
        let mut command = crate::environment::command(fake.to_str().unwrap());
        command.arg("-E").arg(&log);
        proven(&mut command, &log).unwrap()
    };
    assert!(logging(Some(LOGGED)));
    let refused = format!("{LOGGED}Host key verification failed.\n");
    assert!(!logging(Some(&refused)));
    assert!(!logging(None));
}
