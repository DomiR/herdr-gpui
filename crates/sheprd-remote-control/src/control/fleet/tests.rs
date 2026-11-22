//! Shared fixtures: members with fixed keys, hosts, and helpers that sign
//! and carry operations between members' logs.

use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use p256::ecdsa::{SigningKey, VerifyingKey, signature::Signer};

mod access;
mod access_floor;
mod admission;
mod agent_ssh_keys;
mod agents;
mod also;
mod also_checkpoints;
mod base_lifetime;
mod bluetooth;
mod bounds;
mod checkpoint;
mod checkpoint_bytes;
mod convergence;
mod drafting;
mod dry_run;
mod encoding;
mod forgery;
mod hardening;
mod held_access;
mod hosts;
mod lineage;
mod pending;
mod pruning;
mod recovery;
mod removal;
mod returns;
mod rotation;
mod routes;
mod salvage;
mod saved_bounds;
mod saved_checkpoint;
mod settled;
mod vectors;

/// A fixed "now": 2027-01-15.
const NOW: u64 = 1_800_000_000_000;

/// A fixed P-256 key; `seed` picks it.
fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

fn compressed(key: &SigningKey) -> body::PublicKey {
    key.verifying_key()
        .to_sec1_point(true)
        .as_bytes()
        .try_into()
        .unwrap()
}

fn uncompressed(key: &SigningKey) -> [u8; 65] {
    key.verifying_key()
        .to_sec1_point(false)
        .as_bytes()
        .try_into()
        .unwrap()
}

fn raw_signature(key: &SigningKey, message: &[u8]) -> body::RawSignature {
    let signature: p256::ecdsa::Signature = key.sign(message);
    signature.to_bytes().as_slice().try_into().unwrap()
}

/// An `ssh-ed25519` line whose key is 32 bytes of `seed`.
fn ssh_line(seed: u8) -> String {
    let mut blob = Vec::new();
    for part in [b"ssh-ed25519".as_slice(), &[seed; 32]] {
        blob.extend_from_slice(&u32::try_from(part.len()).unwrap().to_be_bytes());
        blob.extend_from_slice(part);
    }
    format!("ssh-ed25519 {}", STANDARD.encode(blob))
}

fn record(host: HostId, seed: u8, rev: u64) -> SelfRecord {
    SelfRecord {
        host,
        rev,
        route: format!("route{seed}"),
        relays: vec![format!("https://relay{seed}.example.org")],
        identity: format!("identity{seed}"),
        direct: vec![format!("192.168.1.{seed}:7443")],
        ssh_host_keys: vec![ssh_line(seed.wrapping_add(100))],
        signed_ms: NOW,
    }
}

/// A host with a certificate authority: a computer member's own (`kind`
/// `Member`) or a Sheprd host's. Returns the CA's key too, to sign later
/// records.
fn ca_host(seed: u8, kind: HostKind) -> (HostIntro, SigningKey) {
    let ca_key = signing_key(seed.wrapping_add(50));
    let key = uncompressed(&ca_key);
    let ca = Ca {
        certificate: [b"cert".as_slice(), &key, &[seed]].concat(),
        key,
    };
    let id = HostId(ca.fingerprint());
    let record = record(id, seed, 1);
    let record_signature = Some(raw_signature(&ca_key, &record.message().unwrap()));
    let intro = HostIntro {
        kind,
        name: format!("host{seed}"),
        user: "me".into(),
        ca: Some(ca),
        record,
        record_signature,
        verified: Verified::BluetoothCode,
    };
    (intro, ca_key)
}

/// A plain host, running the agent.
fn plain_host(seed: u8) -> HostIntro {
    let first = ssh_line(seed.wrapping_add(100));
    let id = HostId(sha256(&[DOMAIN, b"/host", first.as_bytes()]));
    HostIntro {
        kind: HostKind::Agent,
        name: format!("plain{seed}"),
        user: "deploy".into(),
        ca: None,
        record: record(id, seed, 1),
        record_signature: None,
        verified: Verified::KnownHosts,
    }
}

/// A member device: its fleet key and bundle.
#[derive(Clone)]
struct Device {
    key: SigningKey,
    bundle: Bundle,
}

impl Device {
    fn id(&self) -> MemberId {
        self.bundle.id()
    }
}

fn computer(seed: u8) -> Device {
    let key = signing_key(seed);
    let bundle = Bundle {
        key: compressed(&key),
        kind: Kind::Computer,
        name: format!("computer{seed}"),
        dial: compressed(&signing_key(seed.wrapping_add(100))),
        tunnel: [seed; 32],
        ssh: Some(ssh_line(seed)),
        host: Some(ca_host(seed, HostKind::Member).0),
    };
    Device { key, bundle }
}

fn phone(seed: u8) -> Device {
    let key = signing_key(seed);
    let bundle = Bundle {
        key: compressed(&key),
        kind: Kind::Phone,
        name: format!("phone{seed}"),
        dial: compressed(&signing_key(seed.wrapping_add(100))),
        tunnel: [seed; 32],
        ssh: None,
        host: None,
    };
    Device { key, bundle }
}

/// Signs `body` as `by` on top of everything `log` holds.
fn sign(log: &Log, by: &Device, body: Body) -> Op {
    log.draft(by.id(), NOW, body).sign(&by.key).unwrap()
}

/// Signs `body` as `by` on top of everything `log` holds, as it is: an
/// `Access` keeps the `rev` given rather than the one the draft computes.
fn as_drafted(log: &Log, by: &Device, body: Body) -> Op {
    let mut unsigned = log.draft(by.id(), NOW, body.clone());
    unsigned.body = body;
    unsigned.sign(&by.key).unwrap()
}

/// Signs `body` as `by` on exactly `parents`, as a device that had seen
/// only those would.
fn sign_on(log: &Log, by: &Device, parents: &[OpId], seq: u64, body: Body) -> Op {
    let mut parents = parents.to_vec();
    parents.sort_unstable();
    Unsigned {
        fleet: log.fleet(),
        signer: by.id(),
        seq,
        issued_ms: NOW,
        parents,
        body,
    }
    .sign(&by.key)
    .unwrap()
}

/// A new fleet founded by `founder`.
fn found(founder: &Device) -> Log {
    let op = Unsigned {
        fleet: FleetId([0; 32]),
        signer: founder.id(),
        seq: 1,
        issued_ms: NOW,
        parents: Vec::new(),
        body: Body::Found(founder.bundle.clone()),
    }
    .sign(&founder.key)
    .unwrap();
    Log::found(op).unwrap()
}

/// The body that admits `joiner`, with the joiner's own signature.
fn admit_body(log: &Log, joiner: &Device) -> Body {
    let message = joiner.bundle.join_message(&log.fleet()).unwrap();
    Body::Admit(Admit {
        bundle: joiner.bundle.clone(),
        method: Method::Bluetooth,
        joiner_signature: raw_signature(&joiner.key, &message),
    })
}

/// `by` admits `joiner` into `log`.
fn admit(log: &mut Log, by: &Device, joiner: &Device) -> Op {
    let op = sign(log, by, admit_body(log, joiner));
    apply(log, op.clone());
    op
}

fn remove_body(member: &Device) -> Body {
    Body::Remove(Remove {
        member: member.id(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: Reason::Compromised,
    })
}

/// Inserts `op`, which must apply.
fn apply(log: &mut Log, op: Op) {
    assert_eq!(log.insert(op, NOW), Ok(Inserted::Applied));
}

/// Signs `body` as `by` and applies it.
fn act(log: &mut Log, by: &Device, body: Body) -> Op {
    let op = sign(log, by, body);
    apply(log, op.clone());
    op
}

/// Hands `to` everything `from` holds, as sync would.
fn sync(to: &mut Log, from: &Log) {
    let ops: Vec<Op> = from.ops().cloned().collect();
    for (id, result) in to.insert_all(ops, NOW) {
        assert!(result.is_ok(), "{id:?}: {result:?}");
    }
}

/// A copy of `from`, as a member that synced everything would hold.
fn copy(from: &Log) -> Log {
    let founder = from
        .ops()
        .find(|op| matches!(op.body(), Body::Found(_)))
        .unwrap()
        .clone();
    let mut log = Log::found(founder).unwrap();
    sync(&mut log, from);
    log
}
