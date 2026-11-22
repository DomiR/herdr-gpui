//! The fleet's lines in `~/.ssh/authorized_keys`, written the same way by
//! Sheprd's daemon (`ssh_access`) and by this agent:
//!
//! ```text
//! from="127.0.0.1,::1",no-agent-forwarding,no-X11-forwarding ssh-ed25519 AAAA… sheprd-fleet:FLEET:MEMBER
//! ```
//!
//! `FLEET` and `MEMBER` are the first eight bytes of their ids in hex. The
//! line takes connections from loopback only, which only the tunnel's
//! sessions reach from afar. Rewriting touches only lines tagged with the
//! fleet's own marker, refuses a linked file, reads a bounded one, and
//! replaces it whole.

use crate::{Error, Result, files};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Only loopback. Port forwarding stays allowed: Sheprd forwards the ports
/// a remote program listens on.
pub const OPTIONS: &str = r#"from="127.0.0.1,::1",no-agent-forwarding,no-X11-forwarding"#;
/// What precedes the fleet and member ids in a fleet member's line.
pub const FLEET_MARKER: &str = "sheprd-fleet:";
/// The largest `authorized_keys` rewritten; a bigger one is not ours to
/// rewrite.
pub const MAX_FILE: u64 = 1024 * 1024;
/// The longest public key accepted.
pub const MAX_KEY: usize = 256;
const ED25519: &str = "ssh-ed25519";

/// `ssh-ed25519 BASE64` from `text`, its comment dropped, when it is one:
/// the blob must hold the key type and a 32-byte key.
#[must_use]
pub fn ed25519(text: &str) -> Option<String> {
    if text.len() > MAX_KEY {
        return None;
    }
    let mut fields = text.split_ascii_whitespace();
    let (Some(ED25519), Some(blob)) = (fields.next(), fields.next()) else {
        return None;
    };
    let bytes = STANDARD.decode(blob).ok()?;
    // string "ssh-ed25519", then string of the 32-byte key.
    let expected = [
        &(ED25519.len() as u32).to_be_bytes()[..],
        ED25519.as_bytes(),
        &32u32.to_be_bytes(),
    ]
    .concat();
    (bytes.len() == expected.len() + 32 && bytes.starts_with(&expected))
        .then(|| format!("{ED25519} {blob}"))
}

/// The line for a fleet member: `tag` is `FLEET:MEMBER`, `key` a key
/// [`ed25519`] accepted.
#[must_use]
pub fn fleet_line(tag: &str, key: &str) -> String {
    format!("{OPTIONS} {key} {FLEET_MARKER}{tag}")
}

/// The `FLEET:MEMBER` tag of a line the fleet wrote; a line the user
/// wrote, even one ending in the marker, has none.
#[must_use]
pub fn fleet_tag(line: &str) -> Option<&str> {
    if !line.starts_with(OPTIONS) {
        return None;
    }
    line.split_ascii_whitespace()
        .last()?
        .strip_prefix(FLEET_MARKER)
}

/// `~/.ssh/authorized_keys` of the user the agent runs as. Windows keeps
/// administrators' keys elsewhere, and its OpenSSH server is rarely on.
pub fn of_user() -> Result<PathBuf> {
    if cfg!(windows) {
        return Err(Error::Unsupported);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or(Error::NoHome)?;
    Ok(home.join(".ssh").join("authorized_keys"))
}

/// Makes the lines of fleet `fleet` (its short id) in `path` exactly
/// `members`, each `(MEMBER, KEY)`. Lines the user wrote, and other
/// fleets' lines, stay as they are, byte for byte, whatever their
/// encoding. A missing file with nothing to add stays missing; an
/// unchanged one is not written. Blocking.
pub fn set_fleet(path: &Path, fleet: &str, members: &[(String, String)]) -> Result<()> {
    let prefix = format!("{fleet}:");
    let added: Vec<Vec<u8>> = members
        .iter()
        .map(|(member, key)| fleet_line(&format!("{fleet}:{member}"), key).into_bytes())
        .collect();
    rewrite(path, |tag| tag.starts_with(&prefix), &added)
}

/// Removes every fleet's lines from `path`: for an agent that must shut
/// its fleet out and can no longer tell which fleet that is. Blocking.
pub fn clear_fleets(path: &Path) -> Result<()> {
    rewrite(path, |_| true, &[])
}

/// Rewrites `path` without the fleet lines whose tag is `ours`, then with
/// `added`. Lines the user wrote stay byte for byte.
fn rewrite(path: &Path, ours: impl Fn(&str) -> bool, added: &[Vec<u8>]) -> Result<()> {
    // Only a line in UTF-8 can be one the fleet wrote; any other is the
    // user's and is kept as it is.
    let ours = |line: &[u8]| {
        std::str::from_utf8(line)
            .is_ok_and(|line| fleet_tag(line.trim_end_matches('\r')).is_some_and(&ours))
    };
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(Error::Linked {
                path: path.to_owned(),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if added.is_empty() {
                return Ok(());
            }
        }
        Err(source) => {
            return Err(Error::Io {
                path: path.to_owned(),
                source,
            });
        }
    }
    let bytes = files::read_bounded(path, MAX_FILE)?.unwrap_or_default();
    let mut before: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    // The newline that ends the last line starts no line of its own.
    if before.last().is_some_and(|last| last.is_empty()) {
        before.pop();
    }
    let mut lines: Vec<&[u8]> = before.iter().copied().filter(|line| !ours(line)).collect();
    lines.extend(added.iter().map(Vec::as_slice));
    if lines == before {
        return Ok(());
    }
    let mut content = lines.join(&b'\n');
    content.push(b'\n');
    files::write_private(path, &content)
}

#[cfg(test)]
pub(crate) mod tests;
