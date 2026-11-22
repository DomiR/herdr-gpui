//! A host key is confirmed only by a key exchange `ssh` completed with it,
//! never by an exit code or by text the server sent.

use super::*;

/// `ssh -o LogLevel=DEBUG1` against a host that holds the key and refuses
/// every authentication, as OpenSSH prints it.
const EXCHANGED: &str = "\
debug1: Connecting to host [192.0.2.1] port 22.\r
debug1: Server host key: ssh-ed25519 SHA256:abc\r
debug1: Host 'host' is known and matches the ED25519 host key.\r
debug1: ssh_packet_send2_wrapped: resetting send seqnr 3\r
debug1: SSH2_MSG_NEWKEYS sent\r
debug1: expecting SSH2_MSG_NEWKEYS\r
debug1: SSH2_MSG_NEWKEYS received\r
debug1: SSH2_MSG_SERVICE_ACCEPT received\r
debug1: No more authentication methods to try.\r
deploy@host: Permission denied (publickey).\r
";

#[test]
fn a_completed_key_exchange_confirms_the_key() {
    assert!(key_exchange_completed(EXCHANGED.as_bytes()));
    // Without the carriage returns, as on a pipe.
    assert!(key_exchange_completed(
        EXCHANGED.replace('\r', "").as_bytes()
    ));
}

#[test]
fn a_server_saying_permission_denied_before_any_key_exchange_proves_nothing() {
    // The server disconnects before key exchange with text that reads as
    // an authentication failure; OpenSSH exits 255.
    let disconnected = "\
debug1: Connecting to host [192.0.2.1] port 22.\r
Received disconnect from 192.0.2.1 port 22:2: Permission denied (publickey).\r
Disconnected from 192.0.2.1 port 22\r
";
    assert!(!key_exchange_completed(disconnected.as_bytes()));
}

#[test]
fn server_text_naming_the_proof_lines_proves_nothing() {
    for forged in [
        // A disconnect reason spelling the line: prefixed by ssh.
        "Received disconnect from 192.0.2.1 port 22:2: debug1: SSH2_MSG_NEWKEYS received\r\n",
        // A pre-banner line: logged with ssh's own prefix.
        "debug1: kex_exchange_identification: banner line 0: debug1: SSH2_MSG_NEWKEYS received\r\n",
        // The line only as part of another.
        "debug1: SSH2_MSG_NEWKEYS received but not really\n",
        "",
    ] {
        assert!(!key_exchange_completed(forged.as_bytes()), "{forged}");
    }
}

#[test]
fn the_confirming_session_logs_what_proves_the_exchange() {
    let options = confirm_options(Path::new("/tmp/only"), "ssh-ed25519");
    // First, so it wins over anything later or in the user's config.
    assert_eq!(options[..2], ["-o", "LogLevel=DEBUG1"]);
}
