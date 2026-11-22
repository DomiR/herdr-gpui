//! Shared fixtures: devices with fixed keys, a small fleet signed by them,
//! and calls made the way the app makes them, through [`respond`].

use super::*;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::{Value, json};
use sheprd_remote_control::control::fleet::{
    Admit, Body, Bundle, Ca, FleetId, HostIntro, HostKind, Kind, Log, MemberId, Method, Op, Reason,
    Remove, SelfRecord, Unsigned, Verified, sync,
};

mod abi;
mod drafting;
mod replica;
mod vectors;

/// A fixed "now": 2027-01-15.
const NOW: u64 = 1_800_000_000_000;

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_slice(&[seed; 32]).unwrap()
}

fn compressed(key: &SigningKey) -> [u8; 33] {
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

fn raw_signature(key: &SigningKey, message: &[u8]) -> [u8; 64] {
    let signature: Signature = key.sign(message);
    signature.to_bytes().as_slice().try_into().unwrap()
}

/// A Sheprd host with its own certificate authority.
fn sheprd_host(seed: u8, kind: HostKind) -> HostIntro {
    let ca_key = signing_key(seed.wrapping_add(50));
    let key = uncompressed(&ca_key);
    let ca = Ca {
        certificate: [b"cert".as_slice(), &key, &[seed]].concat(),
        key,
    };
    let record = SelfRecord {
        host: sheprd_remote_control::control::fleet::HostId(ca.fingerprint()),
        rev: 1,
        route: format!("route{seed}"),
        relays: vec![format!("https://relay{seed}.example.org")],
        identity: format!("identity{seed}"),
        direct: vec![format!("192.168.1.{seed}:7443")],
        ssh_host_keys: vec![format!("ssh-ed25519 AAAAhost{seed}")],
        signed_ms: NOW,
    };
    let record_signature = Some(raw_signature(&ca_key, &record.message().unwrap()));
    HostIntro {
        kind,
        name: format!("host{seed}"),
        user: "me".into(),
        ca: Some(ca),
        record,
        record_signature,
        verified: Verified::BluetoothCode,
    }
}

#[derive(Clone)]
struct Device {
    key: SigningKey,
    bundle: Bundle,
}

impl Device {
    fn id(&self) -> MemberId {
        self.bundle.id()
    }

    fn hex(&self) -> String {
        self.id().to_string()
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
        ssh: Some(format!("ssh-ed25519 AAAAuser{seed}")),
        host: Some(sheprd_host(seed, HostKind::Member)),
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

/// Signs `body` as `by` on everything `log` holds, and applies it.
fn act(log: &mut Log, by: &Device, body: Body) -> Op {
    let op = log.draft(by.id(), NOW, body).sign(&by.key).unwrap();
    assert!(log.insert(op.clone(), NOW).is_ok());
    op
}

/// A computer's fleet: `a` founded it, admitted computer `b` and phone
/// `p`, and added a Sheprd host. Answers the log as a member daemon holds
/// it, with every operation in the order it signed them.
struct Fleet {
    log: Log,
    ops: Vec<Op>,
    a: Device,
    b: Device,
    p: Device,
    host: String,
}

impl Fleet {
    fn new() -> Self {
        let a = computer(1);
        let b = computer(2);
        let p = phone(3);
        let found = Unsigned {
            fleet: FleetId([0; 32]),
            signer: a.id(),
            seq: 1,
            issued_ms: NOW,
            parents: Vec::new(),
            body: Body::Found(a.bundle.clone()),
        }
        .sign(&a.key)
        .unwrap();
        let mut log = Log::found(found.clone()).unwrap();
        let mut ops = vec![found];
        for joiner in [&b, &p] {
            let message = joiner.bundle.join_message(&log.fleet()).unwrap();
            let body = Body::Admit(Admit {
                bundle: joiner.bundle.clone(),
                method: Method::Qr,
                joiner_signature: raw_signature(&joiner.key, &message),
            });
            ops.push(act(&mut log, &a, body));
        }
        let intro = sheprd_host(9, HostKind::Sheprd);
        let host = intro.record.host.to_string();
        ops.push(act(&mut log, &a, Body::HostAdd(intro)));
        Self {
            log,
            ops,
            a,
            b,
            p,
            host,
        }
    }

    fn fleet(&self) -> String {
        self.log.fleet().to_string()
    }

    fn encoded(&self) -> Vec<String> {
        sync::encode_ops(&self.ops)
    }

    /// `by` removes `member`, on what this log holds.
    fn remove(&mut self, by: &Device, member: &Device) -> Op {
        let body = Body::Remove(Remove {
            member: member.id(),
            also_members: Vec::new(),
            also_hosts: Vec::new(),
            reason: Reason::Lost,
        });
        let by = by.clone();
        let op = act(&mut self.log, &by, body);
        self.ops.push(op.clone());
        op
    }

    /// The phone's replica, started from every operation.
    fn replica(&self) -> Value {
        ok(&json!({
            "call": "apply", "fleet": self.fleet(), "me": self.p.hex(),
            "ops": self.encoded(), "now_ms": NOW,
        }))
    }
}

/// Calls `request`, which must answer `ok`.
fn ok(request: &Value) -> Value {
    let answer = call(request);
    assert!(answer.get("error").is_none(), "{answer}");
    answer["ok"].clone()
}

/// Calls `request`, which must fail; answers its error.
fn refused(request: &Value) -> Value {
    let answer = call(request);
    assert!(answer.get("ok").is_none(), "{answer}");
    answer["error"].clone()
}

fn call(request: &Value) -> Value {
    serde_json::from_slice(&respond(&serde_json::to_vec(request).unwrap())).unwrap()
}

/// The log text a replica answered.
fn log_of(replica: &Value) -> String {
    replica["log"].as_str().unwrap().to_owned()
}

/// Signs the drafts a `draft` call answered with `key`, as the app does.
fn sign_drafts(drafted: &Value, key: &SigningKey) -> Vec<String> {
    drafted["drafts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|draft| {
            let signed = sync::bytes(draft["signed"].as_str().unwrap()).unwrap();
            let signature = raw_signature(key, &signed);
            sync::text(&[signed.as_slice(), &signature].concat())
        })
        .collect()
}
