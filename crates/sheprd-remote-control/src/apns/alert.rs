//! Notifications for the native app, sealed to the device
//! (`docs/ios-app-plan.md`, "Notifications, exactly"). APNs sees only a
//! cover ("Sheprd", "New activity"), a thread and a collapse id; the app's
//! Notification Service Extension opens `sealed` and shows what is inside,
//! with answer buttons for a permission request. The badge travels sealed
//! too: it is this computer's count, and a phone paired with several shows
//! their sum, so APNs must not set it alone.
//!
//! Pane ids are unique only on one computer, so the thread and the collapse
//! id both start with a [`scope`] for the pairing: two computers' `w1:p2`
//! never replace or group with each other, while a newer push for one chat
//! still replaces its older one. The scope is a digest of the notify key
//! the phone gave this computer, which is its own for every computer, and
//! not the computer's fingerprint: APNs and the push relay must not learn a
//! name for the computer, nor tie one computer's pushes to two phones. The
//! extension names the thread itself from the sealed `computer`
//! (`AlertKeys.thread`). The outer thread groups only what iOS shows without
//! running the extension, as when it runs out of time; a push the extension
//! cannot open becomes a fresh cover with no thread, by design, so nothing
//! of the push survives.
//!
//! ```json
//! { "aps": { "alert": { "title": "Sheprd", "body": "New activity" },
//!            "mutable-content": 1, "thread-id": "1f0c9a7e2b4d6e81/w1:p2",
//!            "interruption-level": "time-sensitive", "sound": "default" },
//!   "sealed": "BASE64URL" }
//! ```
//!
//! A clear (the question was answered elsewhere) is a background push,
//! `{"aps": {"content-available": 1}, "sealed": "…"}`, whose
//! sealed `clear` tells the app to remove the chat's notifications.

use super::{Error, Priority, Push, PushType, Result, payload::MAX_PAYLOAD};
use crate::push::{EventId, Kind, Notification};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::PublicKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sheprd_chat_protocol::PermissionOption;

/// The sealed format's version.
pub(crate) const VERSION: u32 = 1;
/// Answer buttons a notification offers at most.
pub(crate) const MAX_OPTIONS: usize = 4;
/// The longest `apns-collapse-id` APNs accepts, in bytes.
const MAX_COLLAPSE_ID: usize = 64;
/// Hex digits of a [`scope`]: 64 bits, which no two of one phone's
/// computers share.
const SCOPE_DIGITS: usize = 16;
/// What a scope's digest starts with, so it is never another digest of the
/// same key.
const SCOPE_INFO: &[u8] = b"sheprd-alert-scope-v1";
/// What APNs and a phone whose extension cannot open the payload show.
pub(super) const COVER_TITLE: &str = "Sheprd";
pub(super) const COVER_BODY: &str = "New activity";

/// What the app's extension opens. Field order is the wire order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Sealed {
    pub(crate) v: u32,
    pub(crate) kind: Kind,
    /// Remove the chat's notifications instead of showing one.
    pub(crate) clear: bool,
    /// The computer's CA fingerprint, 64 uppercase hex digits without
    /// separators, which the app routes answers by.
    pub(crate) computer: String,
    pub(crate) pane: String,
    pub(crate) title: String,
    pub(crate) body: String,
    /// Agents that need an answer or finished unseen.
    pub(crate) badge: u32,
    /// The permission request a question is, for its answer buttons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) permission: Option<SealedPermission>,
    /// The agent asked for attention with `::wizz`: the phone plays its
    /// buzz. Left out otherwise, as older phones expect.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) wizz: bool,
    /// A test push's id, which the phone's extension records on arrival
    /// so the app can show it came. Left out of every other push.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) test: Option<String>,
    /// The notification decision's id ([`EventId`]), the same in every
    /// copy of it: the phone's extension shows one it already showed only
    /// in place of the first, silently. Older daemons leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) event: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SealedPermission {
    pub(crate) id: u64,
    /// At most [`MAX_OPTIONS`], in the agent's order.
    pub(crate) options: Vec<PermissionOption>,
}

impl Sealed {
    /// The sealed side of `notification` from the computer whose CA
    /// fingerprint is `computer` (64 hex digits, no colons).
    pub(crate) fn new(notification: &Notification, computer: String, badge: u32) -> Self {
        Self {
            v: VERSION,
            kind: notification.kind,
            clear: notification.clear,
            computer,
            pane: notification.pane.clone(),
            title: notification.title.clone(),
            body: notification.body.clone(),
            badge,
            permission: notification
                .permission
                .as_ref()
                .map(|permission| SealedPermission {
                    id: permission.id.0,
                    options: permission
                        .options
                        .iter()
                        .take(MAX_OPTIONS)
                        .cloned()
                        .collect(),
                }),
            wizz: notification.wizz,
            test: None,
            event: Some(notification.event.as_str().to_owned()),
        }
    }

    /// The test push `id` the phone asked for, from the computer named
    /// `name` whose fingerprint is `computer`. It carries the computer's
    /// real badge, since the phone takes every alert's count as current.
    pub(crate) fn test(id: String, computer: String, name: &str, badge: u32) -> Self {
        Self {
            v: VERSION,
            kind: Kind::Test,
            clear: false,
            computer,
            pane: TEST_PANE.to_owned(),
            title: "Test push".to_owned(),
            body: format!("From {name}: notifications from this computer reach this phone."),
            badge,
            permission: None,
            wizz: false,
            test: Some(id),
            event: Some(EventId::new().as_str().to_owned()),
        }
    }
}

/// The thread test pushes share, which names no chat.
pub(crate) const TEST_PANE: &str = "test";
/// The longest test id a phone may send.
pub(crate) const MAX_TEST_ID: usize = 64;

/// Whether `id` can be a test push's id: 1 to [`MAX_TEST_ID`] letters,
/// digits, or dashes, as a UUID is.
pub(crate) fn valid_test_id(id: &str) -> bool {
    (1..=MAX_TEST_ID).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[derive(Serialize)]
struct Outer<'a> {
    aps: Aps<'a>,
    sealed: String,
}

#[derive(Serialize)]
struct Aps<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    alert: Option<Cover>,
    #[serde(rename = "mutable-content", skip_serializing_if = "Option::is_none")]
    mutable_content: Option<u8>,
    #[serde(rename = "content-available", skip_serializing_if = "Option::is_none")]
    content_available: Option<u8>,
    #[serde(rename = "thread-id", skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    #[serde(rename = "interruption-level", skip_serializing_if = "Option::is_none")]
    interruption_level: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<&'static str>,
}

#[derive(Serialize)]
struct Cover {
    title: &'static str,
    body: &'static str,
}

/// The push for `sealed`, sealed by `seal`, collapsing under `tag` in
/// `scope` ([`collapse_id`]). The body is shortened until the payload fits APNs's 4 KB.
pub(crate) fn push(
    mut sealed: Sealed,
    scope: &str,
    tag: &str,
    seal: impl Fn(&[u8]) -> Result<Vec<u8>>,
) -> Result<Push> {
    loop {
        let body = outer(&sealed, scope, &seal)?;
        if body.len() <= MAX_PAYLOAD {
            // A test is waited for on the phone, so it may not wait either.
            let urgent =
                (sealed.kind.asks() || sealed.wizz || sealed.kind == Kind::Test) && !sealed.clear;
            return Ok(Push {
                body,
                collapse_id: Some(collapse_id(scope, tag)),
                priority: if urgent {
                    Priority::High
                } else {
                    Priority::Low
                },
                push_type: if sealed.clear {
                    PushType::Background
                } else {
                    PushType::Alert
                },
            });
        }
        if sealed.body.is_empty() {
            return Err(Error::PayloadTooLarge { len: body.len() });
        }
        // Base64 grows the sealed bytes by a third, so the plaintext must
        // lose three quarters of the excess.
        let excess = (body.len() - MAX_PAYLOAD).div_ceil(4) * 3;
        sealed.body = shorten(&sealed.body, excess);
    }
}

/// The outer JSON for `sealed`.
fn outer(
    sealed: &Sealed,
    scope: &str,
    seal: &impl Fn(&[u8]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let plaintext = serde_json::to_vec(sealed)?;
    let thread = thread_id(scope, &sealed.pane);
    let aps = if sealed.clear {
        Aps {
            alert: None,
            mutable_content: None,
            content_available: Some(1),
            thread_id: None,
            interruption_level: None,
            sound: None,
        }
    } else {
        Aps {
            alert: Some(Cover {
                title: COVER_TITLE,
                body: COVER_BODY,
            }),
            mutable_content: Some(1),
            content_available: None,
            thread_id: Some(&thread),
            interruption_level: Some(if sealed.kind.asks() || sealed.wizz {
                "time-sensitive"
            } else {
                "active"
            }),
            sound: Some("default"),
        }
    };
    Ok(serde_json::to_vec(&Outer {
        aps,
        sealed: URL_SAFE_NO_PAD.encode(seal(&plaintext)?),
    })?)
}

/// `text` at least `drop` bytes shorter, cut on a character boundary and
/// ending in `…`; empty when that leaves nothing.
fn shorten(text: &str, drop: usize) -> String {
    const ELLIPSIS: char = '\u{2026}';
    let base = text.strip_suffix(ELLIPSIS).unwrap_or(text);
    let Some(mut end) = base.len().checked_sub(drop + ELLIPSIS.len_utf8()) else {
        return String::new();
    };
    while !base.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return String::new();
    }
    format!("{}{ELLIPSIS}", &base[..end])
}

/// The only text a push for `pane` in `scope` under `tag` carries
/// unsealed: the cover, the thread, how it interrupts, and the collapse id.
/// A push through the relay may carry nothing else in the clear
/// (`sheprd_apns::SealedPush`).
pub(crate) fn clear(scope: &str, pane: &str, tag: &str) -> [String; 7] {
    [
        COVER_TITLE.to_owned(),
        COVER_BODY.to_owned(),
        thread_id(scope, pane),
        "time-sensitive".to_owned(),
        "active".to_owned(),
        "default".to_owned(),
        collapse_id(scope, tag),
    ]
}

/// What a computer's alerts to one phone share in the clear, from `key`,
/// the notify key the phone gave that computer: [`SCOPE_DIGITS`] hex digits
/// of a digest, telling the phone's computers apart without naming any.
pub(crate) fn scope(key: &PublicKey) -> String {
    let mut hash = Sha256::new();
    hash.update(SCOPE_INFO);
    hash.update(key.to_sec1_bytes());
    let digest = hash.finalize();
    digest
        .iter()
        .take(SCOPE_DIGITS / 2)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The notification thread of `pane` in `scope`.
pub(crate) fn thread_id(scope: &str, pane: &str) -> String {
    format!("{scope}/{pane}")
}

/// The collapse id of `tag` in `scope`: the scope, a dash, and `tag`, or,
/// when that passes APNs's 64 bytes, a digest of `tag` instead, so no two
/// long tags share a cut-off prefix.
pub(crate) fn collapse_id(scope: &str, tag: &str) -> String {
    let id = format!("{scope}-{tag}");
    if id.len() <= MAX_COLLAPSE_ID {
        return id;
    }
    let mut digest = URL_SAFE_NO_PAD.encode(Sha256::digest(tag.as_bytes()));
    digest.truncate(MAX_COLLAPSE_ID.saturating_sub(scope.len() + 1));
    format!("{scope}-{digest}")
}
