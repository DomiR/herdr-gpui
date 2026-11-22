//! What Sheprd asks its daemon about the fleet (`Request::Fleet`), and the
//! exchange `sheprd-remote-control fleet apply` runs over SSH on another
//! member or host. Operations travel as their canonical bytes in
//! base64url, so nothing here can change what was signed.

use super::{
    Body, Error, FleetId, HostId, Kind, Log, MemberId, Method, Op, OpId, Reason, Result, State,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// The most operation bytes one answer carries before saying `more`.
pub const MAX_ANSWER_BYTES: usize = 256 * 1024;
/// The largest `fleet apply` request read.
pub const MAX_REQUEST_BYTES: usize = 2 * MAX_ANSWER_BYTES + 64 * 1024;

/// One `fleet apply`: the caller's heads, so the answer carries what it
/// lacks, and the operations it believes the receiver lacks.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Apply {
    /// The fleet, hex; none from a caller asking a host with no fleet yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fleet: Option<String>,
    #[serde(default)]
    pub heads: Vec<String>,
    #[serde(default)]
    pub ops: Vec<String>,
    /// A fresh random value the answering computer signs with its
    /// certificate authority ([`Proof`]), so the caller knows it is a host
    /// the fleet holds before sending it anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// The host the machine on the other end proved it is, hex, when Sheprd
    /// asks its own daemon on that machine's behalf: the daemon exports
    /// nothing to a host the fleet no longer holds, checked under the lock
    /// the export is read under, as the host may be removed while its
    /// answer was on its way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
}

/// An answering computer's proof that it is the host its certificate
/// names: its certificate authority's certificate and key, and that key's
/// signature over [`proof_message`], all base64url.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Proof {
    pub certificate: String,
    pub key: String,
    pub signature: String,
}

/// What a `fleet apply` came to.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    pub fleet: String,
    pub heads: Vec<String>,
    /// What the caller lacks, in graph order.
    pub ops: Vec<String>,
    /// How many of the caller's operations were refused.
    #[serde(default)]
    pub refused: usize,
    /// The answer stopped at [`MAX_ANSWER_BYTES`]: ask again.
    #[serde(default)]
    pub more: bool,
    /// The answering computer's proof, when the caller sent a nonce and it
    /// has a certificate authority to sign with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<Proof>,
}

/// What a computer answering a `fleet apply` signs: the domain, the
/// caller's nonce, then its fleet and its heads, each after its length.
#[must_use]
pub fn proof_message(nonce: &str, fleet: &str, heads: &[String]) -> Vec<u8> {
    let mut message = [super::DOMAIN, b"/sync"].concat();
    for text in std::iter::once(nonce)
        .chain(std::iter::once(fleet))
        .chain(heads.iter().map(String::as_str))
    {
        message.extend_from_slice(&u32::try_from(text.len()).unwrap_or(u32::MAX).to_be_bytes());
        message.extend_from_slice(text.as_bytes());
    }
    message
}

/// Whether `applied` comes from one of `hosts` (host ids, hex): its proof's
/// certificate hashes to one of them, holds the proof's key, and that key
/// signed [`proof_message`] for `nonce`, this answer's fleet, and its
/// heads. A machine that only echoes a fleet id proves nothing.
#[must_use]
pub fn proven(applied: &Applied, nonce: &str, hosts: &[String]) -> bool {
    proven_host(applied, nonce, hosts).is_some()
}

/// Which of `hosts` proved `applied`, as [`proven`] checks it.
#[must_use]
pub fn proven_host(applied: &Applied, nonce: &str, hosts: &[String]) -> Option<String> {
    use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
    let proof = applied.proof.as_ref()?;
    let (Ok(certificate), Ok(key), Ok(signature)) = (
        bytes(&proof.certificate),
        bytes(&proof.key),
        bytes(&proof.signature),
    ) else {
        return None;
    };
    let host = HostId(super::sha256(&[&certificate])).to_string();
    // As `host_relay` checks a carried answer: the key must be the
    // certificate's own.
    if !hosts.contains(&host)
        || key.len() != 65
        || key[0] != 0x04
        || !certificate
            .windows(key.len())
            .any(|window| window == key.as_slice())
    {
        return None;
    }
    let (Ok(key), Ok(signature)) = (
        VerifyingKey::from_sec1_bytes(&key),
        Signature::from_slice(&signature),
    ) else {
        return None;
    };
    let message = proof_message(nonce, &applied.fleet, &applied.heads);
    key.verify(&message, &signature).is_ok().then_some(host)
}

impl Apply {
    /// The heads, read; one that is not an id fails the request.
    pub fn heads(&self) -> Result<Vec<OpId>> {
        self.heads
            .iter()
            .map(|head| OpId::from_hex(head).ok_or(Error::Encoding))
            .collect()
    }

    /// The operations, read.
    pub fn ops(&self) -> Result<Vec<Op>> {
        decode_ops(&self.ops)
    }
}

/// The answer to `apply` from `log`: its heads and what the caller lacks.
#[must_use]
pub fn answer(log: &Log, caller_heads: &[OpId], refused: usize) -> Applied {
    let mut ops = Vec::new();
    let mut bytes = 0;
    let mut more = false;
    for op in log.missing_for(caller_heads) {
        let encoded = op.encode();
        if bytes + encoded.len() > MAX_ANSWER_BYTES {
            more = true;
            break;
        }
        bytes += encoded.len();
        ops.push(URL_SAFE_NO_PAD.encode(encoded));
    }
    Applied {
        fleet: log.fleet().to_string(),
        heads: log.heads().iter().map(ToString::to_string).collect(),
        ops,
        refused,
        more,
        proof: None,
    }
}

/// Operations from base64url text.
pub fn decode_ops(ops: &[String]) -> Result<Vec<Op>> {
    ops.iter()
        .map(|text| {
            let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| Error::Encoding)?;
            Op::decode(&bytes)
        })
        .collect()
}

/// Operations as base64url text.
#[must_use]
pub fn encode_ops<'a>(ops: impl IntoIterator<Item = &'a Op>) -> Vec<String> {
    ops.into_iter()
        .map(|op| URL_SAFE_NO_PAD.encode(op.encode()))
        .collect()
}

/// What Sheprd asks its daemon.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "ask")]
pub enum Ask {
    /// The fleet as this daemon sees it.
    Status,
    /// Founds a fleet with this computer as its only member.
    Found,
    /// This computer's bundle, signed for `fleet`, for a member that is
    /// about to admit it.
    Bundle { fleet: String },
    /// This computer's bundle signed for `fleet` and bound to a Bluetooth
    /// pairing whose token hashes to `pairing`, base64url
    /// (`bluetooth::pairing_digest`), for a claim that joins it.
    Joining { fleet: String, pairing: String },
    /// Admits a member, after a person's approval here.
    Admit {
        bundle: String,
        signature: String,
        method: Method,
    },
    /// Removes a member, and what goes with it.
    Remove {
        member: String,
        #[serde(default)]
        also_members: Vec<String>,
        #[serde(default)]
        also_hosts: Vec<String>,
        reason: Reason,
    },
    /// Shares a host, with members left out.
    AddHost {
        intro: String,
        #[serde(default)]
        deny: Vec<String>,
    },
    /// Allows a computer member on a host, or not.
    Access {
        host: String,
        member: String,
        allow: bool,
    },
    /// Removes a host everywhere.
    RemoveHost { host: String },
    /// Takes operations another member carried, and says what it lacks.
    Apply(Apply),
    /// This computer as a Sheprd host, its record signed by its own
    /// certificate authority, for a member about to share it.
    Intro,
    /// Forgets the fleet here without signing anything: its log, and the
    /// `authorized_keys` lines and relay records it wrote. For a computer
    /// the fleet removed, or a fleet that is gone (`sheprd fleet scrub`).
    /// `session` is the loopback client port of the fleet SSH session that
    /// asked, if one did: only it may finish reading the answer.
    Scrub {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<u16>,
    },
    /// The access file for plain host `host`, for Sheprd to hand its
    /// agent (`fleet apply`): computed here, so the log never leaves the
    /// daemon. `suspended` is what the agent's `status --json` reports
    /// suspending, which the file settles as the log allows.
    AgentAccess {
        host: String,
        #[serde(default)]
        suspended: std::collections::BTreeMap<
            sheprd_relay_agent::access::Id,
            sheprd_relay_agent::fleet::Suspension,
        >,
    },
    /// The phones waiting to join through this computer, for a person to
    /// admit one with [`Ask::Admit`]; none is ever admitted otherwise.
    Joins,
    /// A shared Sheprd host's current `fleet intro`, read over SSH by a
    /// member syncing it: the member carries its record into the log as a
    /// `HostSeen` when it moved, as a host that is no member cannot sign
    /// one. Its certificate authority's signature is what counts.
    Seen { intro: String },
    /// The hosts removed from the fleet ([`removed_hosts`]): Sheprd still
    /// reaches them, to hand each its removal and nothing else.
    RemovedHosts,
    /// What the removed `host`, whose heads are `heads`, lacks of its
    /// removal ([`removal_for`]), as an [`Answer::Applied`].
    RemovalFor { host: String, heads: Vec<String> },
}

/// What the daemon answers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "answer")]
pub enum Answer {
    Status {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Box<super::View>>,
    },
    Bundle {
        bundle: String,
        signature: String,
    },
    Applied(Applied),
    Done,
    /// [`Ask::Joining`]'s answer.
    Joining(super::bluetooth::Joining),
    /// [`Ask::Intro`]'s answer: a `HostIntro`'s canonical bytes,
    /// base64url.
    Intro {
        intro: String,
    },
    /// [`Ask::RemovedHosts`]'s answer: host ids, hex.
    Hosts {
        hosts: Vec<String>,
    },
    /// [`Ask::AgentAccess`]'s answer: the access file, as the agent reads
    /// it on stdin.
    AgentAccess {
        access: String,
    },
    Joins {
        joins: Vec<Join>,
    },
}

/// A phone that asked to join through this computer
/// (`POST /api/v2/fleet/join`), kept in memory for a while.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Join {
    /// The joiner's member id, hex.
    pub member: String,
    /// The name in its bundle.
    pub name: String,
    pub kind: Kind,
    /// Its bundle and its signature over [`super::Bundle::join_message`],
    /// base64url, as [`Ask::Admit`] takes them.
    pub bundle: String,
    pub signature: String,
    /// The paired device that asked, as this computer names it.
    pub device: String,
    /// That device's id, which a pairing approved with "Also add to your
    /// fleet" names, so Sheprd admits only the phone just approved.
    pub device_id: crate::control::DeviceId,
    pub received_ms: u64,
}

/// Reads a member id from hex.
pub fn member(text: &str) -> Result<MemberId> {
    MemberId::from_hex(text).ok_or(Error::Encoding)
}

/// Reads a host id from hex.
pub fn host(text: &str) -> Result<HostId> {
    HostId::from_hex(text).ok_or(Error::Encoding)
}

/// Reads a fleet id from hex.
pub fn fleet(text: &str) -> Result<FleetId> {
    FleetId::from_hex(text).ok_or(Error::Encoding)
}

/// Bytes from base64url.
pub fn bytes(text: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text).map_err(|_| Error::Encoding)
}

/// Text from bytes, base64url.
#[must_use]
pub fn text(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The next `rev` for an `Access` on `host` and `member`: one above what
/// `state` holds.
#[must_use]
pub fn next_rev(state: &State, host: HostId, member: MemberId) -> u64 {
    state
        .access
        .get(&(host, member))
        .map_or(0, |register| register.rev)
        .saturating_add(1)
}

/// Whether `op` removes `host`: a `HostRemove` of it, or a member's
/// removal that takes it along.
fn removes_host(op: &Op, host: &HostId) -> bool {
    match op.body() {
        Body::HostRemove(removed) => removed == host,
        Body::Remove(remove) => remove.also_hosts.contains(host),
        _ => false,
    }
}

/// Every host a `HostRemove`, or a removal's `also_hosts`, took out of
/// `log` and that it holds no longer: what may still prove itself to a
/// member syncing it, to be handed its removal ([`removal_for`]) and
/// nothing else. A removed member's own host is left out: the member's
/// removal reaches it as a member.
#[must_use]
pub fn removed_hosts(log: &Log) -> Vec<String> {
    let live = &log.state().hosts;
    let mut removed = std::collections::BTreeSet::new();
    for op in log.ops() {
        match op.body() {
            Body::HostRemove(host) => {
                removed.insert(*host);
            }
            Body::Remove(remove) => removed.extend(remove.also_hosts.iter().copied()),
            _ => {}
        }
    }
    removed
        .iter()
        .filter(|host| !live.contains_key(host))
        .map(ToString::to_string)
        .collect()
}

/// What a host `log` removed, whose heads are `heads`, lacks of its
/// removal: each operation that removes it and the ancestors of those, in
/// graph order, at most [`MAX_ANSWER_BYTES`] of them (`more` when cut).
/// Nothing made after or beside its removal, so a removed host learns
/// nothing more of the fleet. Its `heads` are the removals' ids. Nothing
/// for a host the fleet still holds.
#[must_use]
pub fn removal_for(log: &Log, host: &HostId, heads: &[OpId]) -> Applied {
    let mut applied = Applied {
        fleet: log.fleet().to_string(),
        ..Applied::default()
    };
    if log.state().hosts.contains_key(host) {
        return applied;
    }
    let removals: Vec<OpId> = log
        .ops()
        .filter(|op| removes_host(op, host))
        .map(Op::id)
        .collect();
    // The removals and their ancestors, walked through their parents.
    let mut history = std::collections::BTreeSet::new();
    let mut walk = removals.clone();
    while let Some(id) = walk.pop() {
        if history.insert(id)
            && let Some(op) = log.get(&id)
        {
            walk.extend(op.parents().iter().copied());
        }
    }
    let mut bytes = 0;
    for op in log
        .missing_for(heads)
        .into_iter()
        .filter(|op| history.contains(&op.id()))
    {
        let encoded = op.encode();
        if bytes + encoded.len() > MAX_ANSWER_BYTES {
            applied.more = true;
            break;
        }
        bytes += encoded.len();
        applied.ops.push(URL_SAFE_NO_PAD.encode(encoded));
    }
    applied.heads = removals.iter().map(ToString::to_string).collect();
    applied
}

/// Whether `log` shares `host`, this computer as a host the fleet holds:
/// what a daemon with no fleet that is not admitted checks before taking a
/// log handed to it.
#[must_use]
pub fn shares(log: &Log, host: &HostId) -> bool {
    log.state().hosts.contains_key(host)
}

/// Whether `log` admits `member`: what a daemon with no fleet checks
/// before taking a log handed to it, so it joins only a fleet that a
/// member admitted it to.
#[must_use]
pub fn admits(log: &Log, member: &MemberId) -> bool {
    log.is_member(member)
        && log
            .ops()
            .any(|op| matches!(op.body(), Body::Admit(admit) if admit.bundle.id() == *member))
}
