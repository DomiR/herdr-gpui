//! The fleet's state, as its log computes it (`fold`), and its canonical
//! bytes, which a checkpoint hashes and a joining member is handed.

use super::{
    Bundle, DOMAIN, Error, HostId, HostIntro, Kind, MAX_COMPUTERS, MAX_HOSTS, MAX_PHONES,
    MAX_STATE, MAX_TOMBSTONES, MemberId, Result, SelfRecord,
    body::RawSignature,
    sha256,
    wire::{Reader, Writer},
};
use std::collections::{BTreeMap, BTreeSet};

/// What a state's bytes start with, after [`DOMAIN`].
const STATE: &[u8] = b"/state";
const MAX_LIVE: usize = MAX_COMPUTERS + MAX_PHONES;
const MAX_ALL_HOSTS: usize = MAX_HOSTS + MAX_COMPUTERS;
const MAX_DEVICE_KEY: usize = 65;
/// As `body.rs` bounds a record's route.
const MAX_ROUTE: usize = 2048;

/// A live member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Member {
    pub bundle: Bundle,
    /// Who admitted it; none for the founder.
    pub admitted_by: Option<MemberId>,
    /// The highest sequence number it signed.
    pub seq: u64,
    /// The sequence number of its own `Rotate` that put `bundle` in
    /// place; 0 for the bundle it was admitted with. Kept in a checkpoint,
    /// so a pruned log and a whole one order its key updates alike.
    pub rotated: u64,
}

/// A live host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Host {
    pub intro: HostIntro,
    /// The member that added it; a computer member adds itself.
    pub added_by: MemberId,
    /// Its latest self-record.
    pub record: SelfRecord,
    /// The CA's signature over `record`; none for a plain host.
    pub record_signature: Option<RawSignature>,
    /// Two members added it with different keys, or its intro names a
    /// relay route an earlier host owns: nothing is spread until a person
    /// picks one.
    pub held: bool,
    /// It signed two different records with one `rev`, or a record
    /// naming a relay route another host owns.
    pub flagged: bool,
}

/// Whether a computer member may reach a host, and the latest `rev`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessRegister {
    pub allow: bool,
    pub rev: u64,
}

/// The fleet, as its operations compute it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct State {
    pub members: BTreeMap<MemberId, Member>,
    /// Every member ever removed; never admitted again.
    pub tombstones: BTreeSet<MemberId>,
    /// Live hosts, computer members' own included.
    pub hosts: BTreeMap<HostId, Host>,
    /// Explicit access; a pair without one is allowed.
    pub access: BTreeMap<(HostId, MemberId), AccessRegister>,
    /// The highest `rev` any `Access` reached: the next is one above the
    /// highest its own history holds, this one included.
    pub access_rev: u64,
    /// Each phone's device key per host.
    pub links: BTreeMap<(MemberId, HostId), Vec<u8>>,
    /// Members that signed two operations neither had seen: their key is
    /// in use in two places. Their `HostAdd` and `Access` are held.
    pub equivocating: BTreeSet<MemberId>,
    /// Each relay route a live host names, and the host that owns it: the
    /// first to claim it (`routes`).
    pub routes: BTreeMap<String, HostId>,
}

impl State {
    /// Whether computer member `member` may reach host `host`: the host
    /// not held, and [`State::granted`].
    #[must_use]
    pub fn allowed(&self, host: &HostId, member: &MemberId) -> bool {
        self.hosts.get(host).is_some_and(|entry| !entry.held) && self.granted(host, member)
    }

    /// Whether the fleet grants computer member `member` host `host`, a
    /// hold aside: both live, the member not the host itself, and no deny.
    /// What a plain host's access file says of each member, so a hold,
    /// which the file carries apart (`paused`), never reads as a deny.
    #[must_use]
    pub fn granted(&self, host: &HostId, member: &MemberId) -> bool {
        let (Some(_), Some(person)) = (self.hosts.get(host), self.members.get(member)) else {
            return false;
        };
        let own = person
            .bundle
            .host
            .as_ref()
            .and_then(|intro| intro.id().ok())
            .is_some_and(|id| id == *host);
        person.bundle.kind == Kind::Computer
            && !own
            && self
                .access
                .get(&(*host, *member))
                .is_none_or(|register| register.allow)
    }

    /// The computer members allowed on `host`.
    #[must_use]
    pub fn grants(&self, host: &HostId) -> Vec<MemberId> {
        self.members
            .keys()
            .filter(|member| self.allowed(host, member))
            .copied()
            .collect()
    }

    /// The canonical bytes a checkpoint hashes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.raw(DOMAIN);
        writer.raw(STATE);
        writer.count(self.members.len())?;
        for (id, member) in &self.members {
            writer.raw(&id.0);
            member.bundle.write(&mut writer)?;
            writer.option(member.admitted_by.as_ref(), |writer, by| {
                writer.raw(&by.0);
                Ok(())
            })?;
            writer.u64(member.seq);
            writer.u64(member.rotated);
        }
        writer.count(self.tombstones.len())?;
        self.tombstones.iter().for_each(|id| writer.raw(&id.0));
        writer.count(self.hosts.len())?;
        for (id, host) in &self.hosts {
            writer.raw(&id.0);
            host.intro.write(&mut writer)?;
            writer.raw(&host.added_by.0);
            host.record.write(&mut writer)?;
            writer.option(host.record_signature.as_ref(), |writer, signature| {
                writer.raw(signature);
                Ok(())
            })?;
            writer.bool(host.held);
            writer.bool(host.flagged);
        }
        writer.count(self.access.len())?;
        for ((host, member), register) in &self.access {
            writer.raw(&host.0);
            writer.raw(&member.0);
            writer.bool(register.allow);
            writer.u64(register.rev);
        }
        writer.u64(self.access_rev);
        writer.count(self.links.len())?;
        for ((member, host), key) in &self.links {
            writer.raw(&member.0);
            writer.raw(&host.0);
            writer.bytes(key)?;
        }
        writer.count(self.equivocating.len())?;
        self.equivocating.iter().for_each(|id| writer.raw(&id.0));
        writer.count(self.routes.len())?;
        for (route, owner) in &self.routes {
            writer.text(route)?;
            writer.raw(&owner.0);
        }
        let bytes = writer.into_bytes();
        if bytes.len() > MAX_STATE {
            return Err(Error::Bounds);
        }
        Ok(bytes)
    }

    /// The hash a checkpoint names.
    pub fn hash(&self) -> Result<[u8; 32]> {
        Ok(sha256(&[&self.encode()?]))
    }

    /// Reads a state, refusing anything out of order, out of bounds, or
    /// that would encode differently.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_STATE {
            return Err(Error::Bounds);
        }
        let mut reader = Reader::new(bytes);
        if reader.raw(DOMAIN.len())? != DOMAIN || reader.raw(STATE.len())? != STATE {
            return Err(Error::Encoding);
        }
        let mut state = Self::default();
        for _ in 0..reader.count(MAX_LIVE)? {
            let id = MemberId(reader.array()?);
            let member = Member {
                bundle: Bundle::read(&mut reader)?,
                admitted_by: reader.option(|reader| reader.array().map(MemberId))?,
                seq: reader.u64()?,
                rotated: reader.u64()?,
            };
            if member.bundle.id() != id {
                return Err(Error::Invalid);
            }
            state.members.insert(id, member);
        }
        for _ in 0..reader.count(MAX_TOMBSTONES)? {
            state.tombstones.insert(MemberId(reader.array()?));
        }
        for _ in 0..reader.count(MAX_ALL_HOSTS)? {
            let id = HostId(reader.array()?);
            let host = Host {
                intro: HostIntro::read(&mut reader)?,
                added_by: MemberId(reader.array()?),
                record: SelfRecord::read(&mut reader)?,
                record_signature: reader.option(Reader::array)?,
                held: reader.bool()?,
                flagged: reader.bool()?,
            };
            if host.intro.id()? != id {
                return Err(Error::Invalid);
            }
            state.hosts.insert(id, host);
        }
        for _ in 0..reader.count(MAX_ALL_HOSTS * MAX_COMPUTERS)? {
            let key = (HostId(reader.array()?), MemberId(reader.array()?));
            let register = AccessRegister {
                allow: reader.bool()?,
                rev: reader.u64()?,
            };
            state.access.insert(key, register);
        }
        state.access_rev = reader.u64()?;
        for _ in 0..reader.count(MAX_ALL_HOSTS * MAX_PHONES)? {
            let key = (MemberId(reader.array()?), HostId(reader.array()?));
            state.links.insert(key, reader.bytes(MAX_DEVICE_KEY)?);
        }
        for _ in 0..reader.count(MAX_LIVE + MAX_TOMBSTONES)? {
            state.equivocating.insert(MemberId(reader.array()?));
        }
        // A host's intro and its record name one route each.
        for _ in 0..reader.count(MAX_ALL_HOSTS * 2)? {
            let route = reader.text(MAX_ROUTE)?;
            state.routes.insert(route, HostId(reader.array()?));
        }
        reader.finish()?;
        // Maps drop duplicates and sort; the bytes must already have been
        // that way.
        if state.encode()? != bytes {
            return Err(Error::Encoding);
        }
        Ok(state)
    }
}
