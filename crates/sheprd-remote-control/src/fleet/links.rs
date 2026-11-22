//! Phones on this computer's API through the fleet (`docs/relay-plan.md`,
//! What joining grants, A phone that joins): a live phone member that
//! signed a `Link` naming this computer's own host, with a device key it
//! made for it, is a paired device here, as a host link makes one.
//!
//! - The device is the API only: no SSH key and no GitHub sharing. Its
//!   relay keys are the dial and tunnel keys of the phone's own bundle,
//!   which the phone signed when it joined, so the relays let it through
//!   and the tunnel admits it to the API at once; a `Rotate` moves them.
//!   Keys are public, so keys that are another device's or another
//!   member's are never set: a bundle naming them must not divert their
//!   sessions. The device is still admitted, reachable by its other paths.
//! - Its id comes from its key ([`device_id`]), so the phone knows it
//!   without asking, and a phone that links another key is another device:
//!   the old one goes.
//! - It goes when the phone member is removed or suspended, or its link
//!   does.
//! - A device this module admitted that is gone from `devices.json` while
//!   its link stands was revoked here by hand, a local refusal: it is not
//!   admitted again, even after a suspension of the phone comes and goes.
//!   `fleet/links.json` keeps the ids admitted, with the relay keys last
//!   set on each, and those refused, so this module only ever removes its
//!   own devices, and leaves relay keys a phone sent itself.

mod error;

pub use error::{Error, Result};

use super::{Fleet, grants::own_host};
use crate::auth::{
    NewDevice, PublicKey,
    devices::{
        RelayKeys,
        relay_keys::{FLEET_ID, FLEET_VIA},
    },
};
use crate::control::{
    DeviceId,
    fleet::{Bundle, Kind, MemberId, State as Log},
};
use crate::desktop::files;
use crate::web::State;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The ids of the devices links admitted.
const FILE: &str = "fleet/links.json";

/// The device id a phone's link key `key` (P-256, SEC1, either form) gets
/// on the host it names, with the key as devices keep it: `fleet-` and the
/// first 16 bytes of the SHA-256 of the key, uncompressed, in hex.
pub(crate) fn device_id(key: &[u8]) -> Option<(DeviceId, PublicKey)> {
    let point = p256::ecdsa::VerifyingKey::from_sec1_bytes(key).ok()?;
    let uncompressed = point.to_sec1_point(false);
    let hash = Sha256::digest(uncompressed.as_bytes());
    let id: String = hash[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let public_key = PublicKey::parse(&crate::auth::encode(uncompressed.as_bytes())).ok()?;
    Some((DeviceId(format!("{FLEET_ID}{id}")), public_key))
}

pub(super) struct Wanted {
    public_key: PublicKey,
    name: String,
    /// The phone member's bundle keys, when they are no other member's.
    relay: Option<RelayKeys>,
}

/// Admits the devices the log links to this computer and removes those it
/// no longer does; whether any changed. Blocking: it writes
/// `devices.json` and `fleet/links.json`.
pub(crate) fn apply(state: &State, fleet: &Fleet, now_ms: u64) -> Result<bool> {
    // Held from reading the log to writing the devices, as a scrub holds
    // it while it forgets the fleet: no scrub runs in between, so none can
    // be undone by devices admitted from the log it just forgot.
    let _writing = fleet.writing();
    let (wanted, linked, via) = wanted(fleet);
    let file = fleet.state_dir().join(FILE);
    let stored = read(&file)?;
    let mut before = stored.clone();
    let mut kept = Kept::default();
    let mut changed = false;
    let mut auth = state.auth();
    // A device this module was adding when a stop cut it short (`adding`,
    // written before the device) is its own, with the keys it meant to set.
    for (id, relay) in &before.adding {
        if auth.devices().get(&DeviceId(id.clone())).is_some() {
            before
                .admitted
                .entry(id.clone())
                .or_insert_with(|| relay.clone());
        }
    }
    // A device marked as the fleet's (its id and `via`) that the file does
    // not name at all was added before any of it landed: it is this
    // module's, so a removal still reaches it, but its relay keys may be
    // ones the phone sent itself since, so they are not taken for this
    // module's (`None`).
    for record in auth.devices().records() {
        let marked = record.id.0.starts_with(FLEET_ID)
            && record
                .via
                .as_deref()
                .is_some_and(|via| via.starts_with(FLEET_VIA));
        if marked && !before.refused.contains(&record.id.0) {
            before.admitted.entry(record.id.0.clone()).or_insert(None);
        }
    }
    // A device this module admitted that is gone from `devices.json` was
    // revoked here by hand: a local refusal, kept while its link stands,
    // suspended or not, so a suspension lifted later never brings it back.
    // One this module set out to revoke itself (`revoking`, written before
    // the revocation) is not a refusal, whichever write a stop cut short.
    kept.refused = before
        .admitted
        .keys()
        .filter(|id| {
            !before.revoking.contains(*id) && auth.devices().get(&DeviceId((*id).clone())).is_none()
        })
        .chain(&before.refused)
        .filter(|id| linked.contains(*id))
        .cloned()
        .collect();
    let gone: Vec<String> = before
        .admitted
        .keys()
        .filter(|id| !wanted.contains_key(*id))
        .cloned()
        .collect();
    // The relay keys each kept device moves to: those of its bundle, while
    // the keys it holds are ones this module set (`admitted`), or was
    // setting when a stop cut it short (`setting`); keys the phone sent
    // itself (`POST /api/relay`) stay.
    let mut moves: BTreeMap<String, Option<RelayKeys>> = BTreeMap::new();
    for (id, device) in &wanted {
        let key = DeviceId(id.clone());
        let (Some(record), Some(set)) = (auth.devices().get(&key), before.admitted.get(id)) else {
            continue;
        };
        if kept.refused.contains(id) {
            continue;
        }
        let relay = device
            .relay
            .clone()
            .filter(|keys| !auth.devices().relay_taken(&key, keys));
        let ours = record.relay == *set || before.setting.get(id) == Some(&record.relay);
        if ours && record.relay != relay {
            moves.insert(id.clone(), relay);
        }
    }
    // The devices to add, with the relay keys each gets.
    let adds: BTreeMap<String, Option<RelayKeys>> = wanted
        .iter()
        .filter(|(id, _)| {
            !kept.refused.contains(*id) && auth.devices().get(&DeviceId((*id).clone())).is_none()
        })
        .map(|(id, device)| {
            let key = DeviceId(id.clone());
            let relay = device
                .relay
                .clone()
                .filter(|keys| !auth.devices().relay_taken(&key, keys));
            (id.clone(), relay)
        })
        .collect();
    // What is about to change is written first, so a stop between this
    // file and `devices.json` leaves both readable as this module's work.
    if !gone.is_empty() || !moves.is_empty() || !adds.is_empty() {
        let mut intent = stored.clone();
        intent.revoking.extend(gone.iter().cloned());
        intent.setting.extend(moves.clone());
        intent.adding.extend(adds.clone());
        write(&file, &intent)?;
    }
    for id in &gone {
        changed |= auth.revoke(&DeviceId(id.clone()))?;
    }
    for (id, device) in &wanted {
        if kept.refused.contains(id) {
            continue;
        }
        let key = DeviceId(id.clone());
        if auth.devices().get(&key).is_some() {
            let Some(set) = before.admitted.get(id) else {
                // A device this module did not admit is not its own.
                continue;
            };
            match moves.get(id) {
                Some(relay) => {
                    changed |= auth
                        .devices_mut()
                        .replace_relay(&key, relay.clone(), now_ms)?;
                    kept.admitted.insert(id.clone(), relay.clone());
                }
                None => {
                    kept.admitted.insert(id.clone(), set.clone());
                }
            }
            continue;
        }
        let Some(relay) = adds.get(id).cloned() else {
            continue;
        };
        kept.admitted.insert(id.clone(), relay.clone());
        auth.devices_mut().add_with(
            NewDevice {
                app: None,
                fleet: None,
                id: key,
                name: device.name.clone(),
                public_key: device.public_key.clone(),
                relay,
                ssh: None,
            },
            now_ms,
            Some(via.clone()),
            Vec::new(),
            // Sharing GitHub starts off on hosts the phone did not pair
            // with directly.
            false,
        )?;
        changed = true;
    }
    drop(auth);
    if kept != stored {
        write(&file, &kept)?;
    }
    if changed {
        tracing::info!(
            devices = kept.admitted.len(),
            "fleet links changed this computer's devices"
        );
    }
    Ok(changed)
}

/// Revokes every phone device the fleet admitted here and forgets the
/// record of them, as a scrub does: no phone keeps API access through a
/// fleet this computer forgot. Devices are found both in
/// `fleet/links.json` and by their mark (a `fleet-` id with the fleet
/// `via`), so a damaged or missing file still lets them go. Blocking.
pub(crate) fn forget(auth: &mut crate::auth::Auth, state_dir: &std::path::Path) -> Result<()> {
    let file = state_dir.join(FILE);
    // A file that does not read names nothing; the marks still do.
    let kept = read(&file).unwrap_or_default();
    let mut ids: BTreeSet<String> = kept
        .admitted
        .into_keys()
        .chain(kept.setting.into_keys())
        .chain(kept.revoking)
        .collect();
    ids.extend(
        auth.devices()
            .records()
            .iter()
            .filter(|record| {
                record.id.0.starts_with(FLEET_ID)
                    && record
                        .via
                        .as_deref()
                        .is_some_and(|via| via.starts_with(FLEET_VIA))
            })
            .map(|record| record.id.0.clone()),
    );
    for id in ids {
        let device = DeviceId(id);
        if auth.devices().get(&device).is_some() {
            auth.revoke(&device)?;
        }
    }
    // The record goes only once every device it named is gone.
    match std::fs::remove_file(&file) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(crate::desktop::Error::File {
            operation: "remove",
            path: file,
            source,
        }
        .into()),
    }
}

fn write(file: &std::path::Path, kept: &Kept) -> Result<()> {
    let json = serde_json::to_vec(kept).map_err(|_| crate::desktop::Error::RecordFormat)?;
    files::write_private(file, &json)?;
    Ok(())
}

/// What `fleet/links.json` keeps.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct Kept {
    /// The devices this module admitted, with the relay keys it last set
    /// on each.
    admitted: BTreeMap<String, Option<RelayKeys>>,
    /// Devices revoked here by hand while their link stood: never admitted
    /// again while it stands, whatever suspensions come and go.
    #[serde(default)]
    refused: BTreeSet<String>,
    /// Devices this module is revoking itself, written before the
    /// revocation, so one a stop cut short is not taken for a refusal.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    revoking: BTreeSet<String>,
    /// Relay keys this module is moving devices to, written before the
    /// move, so keys a stop left on a device still read as its own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    setting: BTreeMap<String, Option<RelayKeys>>,
    /// Devices this module is adding, with the relay keys it gives each,
    /// written before the device, so one a stop cut short is still its
    /// own, keys included.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    adding: BTreeMap<String, Option<RelayKeys>>,
}

/// What this module kept. Older files read too: a map of admitted ids to
/// relay keys, or before that a list of ids, with no refusals recorded.
fn read(file: &std::path::Path) -> Result<Kept> {
    let Some(bytes) = files::read_bounded(file, 64 * 1024)? else {
        return Ok(Kept::default());
    };
    if let Ok(kept) = serde_json::from_slice(&bytes) {
        return Ok(kept);
    }
    if let Ok(admitted) = serde_json::from_slice(&bytes) {
        return Ok(Kept {
            admitted,
            ..Kept::default()
        });
    }
    let ids: BTreeSet<String> = serde_json::from_slice(&bytes).unwrap_or_default();
    Ok(Kept {
        admitted: ids.into_iter().map(|id| (id, None)).collect(),
        ..Kept::default()
    })
}

/// The devices the log links to this computer, by id, and what they say
/// vouched for them.
fn wanted(fleet: &Fleet) -> (BTreeMap<String, Wanted>, BTreeSet<String>, String) {
    let suspended = fleet.suspended();
    let inner = fleet.inner();
    let Some(log) = inner.log.as_ref() else {
        return (BTreeMap::new(), BTreeSet::new(), String::new());
    };
    let via = format!("{FLEET_VIA}{}", log.fleet().short());
    let (wanted, linked) = wanted_in(
        log.state(),
        &inner.proven,
        &fleet.me(),
        fleet.host(),
        &suspended,
    );
    (wanted, linked, via)
}

/// What `state` links to `me`'s own host: the devices wanted now, and the
/// ids of every device it links there, suspended or held ones included.
pub(super) fn wanted_in(
    state: &Log,
    proven: &BTreeMap<MemberId, BTreeSet<[u8; 32]>>,
    me: &MemberId,
    // This computer's host as a shared host, when it joined as one.
    hosted: Option<crate::control::fleet::HostId>,
    suspended: &[MemberId],
) -> (BTreeMap<String, Wanted>, BTreeSet<String>) {
    let Some(host) = own_host(state, me, hosted) else {
        return (BTreeMap::new(), BTreeSet::new());
    };
    // While this computer's own host is held (two members claim it), no
    // phone keeps access through a link to it: fail closed until a person
    // settles which one it is. Its links still count as linked, so a
    // refusal made here outlives the hold.
    let held = state.hosts.get(&host).is_none_or(|entry| entry.held);
    let phones: Vec<(&MemberId, &Vec<u8>)> = state
        .links
        .iter()
        .filter(|((phone, linked), _)| {
            *linked == host
                && state
                    .members
                    .get(phone)
                    .is_some_and(|member| member.bundle.kind == Kind::Phone)
        })
        .map(|((phone, _), key)| (phone, key))
        .collect();
    // Every device the log links here, suspended phones' included.
    let linked = phones
        .iter()
        .filter_map(|(_, key)| device_id(key).map(|(id, _)| id.0))
        .collect();
    let wanted = phones
        .into_iter()
        .filter(|(phone, _)| !held && !suspended.contains(phone))
        .filter_map(|(phone, key)| {
            let member = state.members.get(phone)?;
            let (id, public_key) = device_id(key)?;
            let name = crate::auth::device_name(&member.bundle.name).ok()?;
            let relay = (!members_hold(state, proven, phone, &member.bundle))
                .then(|| bundle_keys(&member.bundle));
            Some((
                id.0,
                Wanted {
                    public_key,
                    name,
                    relay,
                },
            ))
        })
        .collect();
    (wanted, linked)
}

/// A bundle's relay keys as devices keep them, in the encodings
/// `POST /api/relay` takes.
fn bundle_keys(bundle: &Bundle) -> RelayKeys {
    RelayKeys {
        dial: crate::auth::encode(&bundle.dial),
        tunnel: crate::auth::encode(&bundle.tunnel),
    }
}

/// Whether another live member's bundle, or a tunnel key another member
/// proved here, is `bundle`'s dial or tunnel key. The log already refuses
/// such a bundle among live members; this also covers keys a member proved
/// and rotated away from.
fn members_hold(
    state: &Log,
    proven: &BTreeMap<MemberId, BTreeSet<[u8; 32]>>,
    phone: &MemberId,
    bundle: &Bundle,
) -> bool {
    state.members.iter().any(|(id, member)| {
        id != phone && (member.bundle.dial == bundle.dial || member.bundle.tunnel == bundle.tunnel)
    }) || proven
        .iter()
        .any(|(id, keys)| id != phone && keys.contains(&bundle.tunnel))
}
