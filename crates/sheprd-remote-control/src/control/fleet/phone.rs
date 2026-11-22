//! What a phone member is told about the fleet beside its view: what it
//! signs its next operation on, the hosts it linked and the keys those
//! links name, where hosts take direct connections, and each linked Sheprd
//! host's record as its certificate authority signed it. A member daemon
//! answers it (`GET /api/v2/fleet`), and the phone computes the same from
//! its own copy of the log, so neither has to take the other's word.

use super::{Body, HostId, HostKind, Log, MemberId, sync};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What `member` is told, when it is live in the log.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Linked {
    /// What the member signs next on, when it is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<Next>,
    /// The hosts the member linked a device key for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    /// The device key each of those links names, base64url, by host: what
    /// the phone checks against the key it made before keeping a host, as
    /// a link the fleet holds may be an earlier one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub link_keys: BTreeMap<String, String>,
    /// Where each Sheprd host and computer member says it takes direct
    /// connections, by host: what a phone tries before its relays.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reach: Vec<Reach>,
    /// Each Sheprd host the member linked: its latest record as its
    /// certificate authority signed it, which the phone checks against the
    /// fingerprint it pins before dialing its relays with its bundle's keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<Route>,
}

/// What a member signs its next operation on.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Next {
    pub fleet: String,
    pub seq: u64,
    pub parents: Vec<String>,
}

/// A linked host's latest record, as its certificate authority signed it,
/// so the phone checks it against the fingerprint it pins rather than
/// taking any computer's word for where the host is. Bytes in base64url.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub host: String,
    /// The CA's certificate, DER: its SHA-256 is the host's id.
    pub certificate: String,
    /// The CA's key, SEC1 uncompressed, which appears in the certificate.
    pub key: String,
    /// What the CA signed (`SelfRecord::message`).
    pub record: String,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Reach {
    pub host: String,
    pub direct: Vec<String>,
}

impl Linked {
    /// What `log` tells `member`; nothing but where hosts are reached when
    /// it is not live there.
    #[must_use]
    pub fn of(log: &Log, member: Option<&MemberId>, now_ms: u64) -> Self {
        let state = log.state();
        let live = member.filter(|member| log.is_member(member));
        let next = live.map(|member| {
            // The body is not signed here: only its sequence number and
            // parents are read.
            let draft = log.draft(*member, now_ms, Body::HostRemove(HostId([0; 32])));
            Next {
                fleet: draft.fleet.to_string(),
                seq: draft.seq,
                parents: draft.parents.iter().map(ToString::to_string).collect(),
            }
        });
        let linked: Vec<HostId> = live
            .map(|member| {
                state
                    .links
                    .keys()
                    .filter(|(phone, _)| phone == member)
                    .map(|(_, host)| *host)
                    .collect()
            })
            .unwrap_or_default();
        let routes = linked
            .iter()
            .filter_map(|id| {
                let host = state.hosts.get(id)?;
                // A plain host has no certificate authority to pin, and no API.
                let ca = host.intro.ca.as_ref()?;
                Some(Route {
                    host: id.to_string(),
                    certificate: sync::text(&ca.certificate),
                    key: sync::text(&ca.key),
                    record: sync::text(&host.record.message()?),
                    signature: sync::text(host.record_signature.as_ref()?),
                })
            })
            .collect();
        let link_keys = live
            .map(|member| {
                state
                    .links
                    .iter()
                    .filter(|((phone, _), _)| phone == member)
                    .map(|((_, host), key)| (host.to_string(), sync::text(key)))
                    .collect()
            })
            .unwrap_or_default();
        let reach = state
            .hosts
            .iter()
            .filter(|(_, host)| {
                host.intro.kind != HostKind::Agent && !host.record.direct.is_empty()
            })
            .map(|(id, host)| Reach {
                host: id.to_string(),
                direct: host.record.direct.clone(),
            })
            .collect();
        Self {
            next,
            links: linked.iter().map(ToString::to_string).collect(),
            link_keys,
            reach,
            routes,
        }
    }
}
