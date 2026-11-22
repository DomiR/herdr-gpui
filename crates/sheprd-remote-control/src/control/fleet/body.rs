//! What each operation says, and its canonical bytes. Every type checks
//! its own bounds and shape when read and before it is written, so a
//! malformed body never reaches the fold.

use super::{
    DOMAIN, Error, HostId, MAX_ALSO_MEMBERS, MAX_HOSTS, MemberId, Result, sha256,
    wire::{Reader, Writer},
};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::{Deserialize, Serialize};

/// A P-256 public key, SEC1 compressed.
pub type PublicKey = [u8; 33];
/// A P-256 ECDSA signature, `r` then `s`.
pub type RawSignature = [u8; 64];

/// What a joining member signs, before the fleet id and its bundle.
const JOIN: &[u8] = b"/join";
/// What a host signs, before its self-record.
const SELF_RECORD: &[u8] = b"/self";
/// What a plain host's id hashes, before its first SSH host key.
const HOST: &[u8] = b"/host";

const MAX_NAME: usize = 64;
const MAX_USER: usize = 64;
const MAX_SSH_KEY: usize = 256;
const MAX_CERTIFICATE: usize = 8192;
const MAX_FIELD: usize = 2048;
const MAX_RELAYS: usize = 16;
const MAX_DIRECT: usize = 8;
const MAX_ADDRESS: usize = 128;
const MAX_HOST_KEYS: usize = 4;
const MAX_HOST_KEY: usize = 1024;
const MAX_DEVICE_KEY: usize = 65;

macro_rules! byte_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $value:literal,)* }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant = $value,)* }

        impl $name {
            fn read(reader: &mut Reader<'_>) -> Result<Self> {
                match reader.u8()? {
                    $($value => Ok(Self::$variant),)*
                    _ => Err(Error::Encoding),
                }
            }

            fn write(self, writer: &mut Writer) {
                writer.u8(self as u8);
            }
        }
    };
}

byte_enum! {
    /// What kind of device a member is.
    Kind { Computer = 1, Phone = 2, }
}

byte_enum! {
    /// How an approval proved the two devices were the right ones.
    Method { Bluetooth = 1, Ssh = 2, Qr = 3, }
}

byte_enum! {
    /// Why a member was removed; decides nothing in the fold.
    Reason { Lost = 1, Retired = 2, Compromised = 3, Left = 4, }
}

byte_enum! {
    /// What runs on a host.
    HostKind { Member = 1, Sheprd = 2, Agent = 3, }
}

byte_enum! {
    /// How the member that added a host checked its SSH host keys.
    Verified { KnownHosts = 1, BluetoothCode = 2, FirstUseApproved = 3, }
}

/// What a member publishes about itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bundle {
    /// Its fleet key.
    pub key: PublicKey,
    pub kind: Kind,
    pub name: String,
    /// Its relay dial key.
    pub dial: PublicKey,
    /// Its tunnel static key (X25519).
    pub tunnel: [u8; 32],
    /// A computer's `ssh-ed25519 BASE64` key; none for a phone.
    pub ssh: Option<String>,
    /// A computer as a host for the others; none for a phone.
    pub host: Option<HostIntro>,
}

impl Bundle {
    /// The member this bundle describes.
    #[must_use]
    pub fn id(&self) -> MemberId {
        MemberId(sha256(&[&self.key]))
    }

    /// Its canonical bytes, as an `Admit` carries it.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        self.write(&mut writer)?;
        Ok(writer.into_bytes())
    }

    /// Reads a bundle from [`Bundle::to_bytes`], strictly.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let bundle = Self::read(&mut reader)?;
        reader.finish()?;
        Ok(bundle)
    }

    pub(super) fn verifying_key(&self) -> Result<VerifyingKey> {
        VerifyingKey::from_sec1_bytes(&self.key).map_err(|_| Error::Invalid)
    }

    /// What the joining member signs: that it holds this bundle's key, for
    /// this fleet.
    #[must_use]
    pub fn join_message(&self, fleet: &super::FleetId) -> Option<Vec<u8>> {
        let mut writer = Writer::new();
        writer.raw(DOMAIN);
        writer.raw(JOIN);
        writer.raw(&fleet.0);
        self.write(&mut writer).ok()?;
        Some(writer.into_bytes())
    }

    pub(super) fn check(&self) -> Result<()> {
        point(&self.key)?;
        point(&self.dial)?;
        name(&self.name, MAX_NAME)?;
        match (self.kind, &self.ssh, &self.host) {
            (Kind::Computer, Some(ssh), Some(host)) if host.kind == HostKind::Member => {
                ssh_key(ssh, MAX_SSH_KEY)?;
                host.check()
            }
            (Kind::Phone, None, None) => Ok(()),
            _ => Err(Error::Invalid),
        }
    }

    pub(super) fn write(&self, writer: &mut Writer) -> Result<()> {
        self.check()?;
        writer.raw(&self.key);
        self.kind.write(writer);
        writer.text(&self.name)?;
        writer.raw(&self.dial);
        writer.raw(&self.tunnel);
        writer.option(self.ssh.as_ref(), |writer, ssh| writer.text(ssh))?;
        writer.option(self.host.as_ref(), |writer, host| host.write(writer))
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Result<Self> {
        let bundle = Self {
            key: reader.array()?,
            kind: Kind::read(reader)?,
            name: reader.text(MAX_NAME)?,
            dial: reader.array()?,
            tunnel: reader.array()?,
            ssh: reader.option(|reader| reader.text(MAX_SSH_KEY))?,
            host: reader.option(HostIntro::read)?,
        };
        bundle.check()?;
        Ok(bundle)
    }
}

/// A Sheprd host's certificate authority, as pinned when it was added.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ca {
    /// The certificate, DER; its SHA-256 is the fingerprint phones pin.
    pub certificate: Vec<u8>,
    /// Its key, SEC1 uncompressed, which signs the host's self-records.
    pub key: [u8; 65],
}

impl Ca {
    /// The fingerprint, and so the host's id.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        sha256(&[&self.certificate])
    }

    fn check(&self) -> Result<()> {
        // As `host_relay` checks a carried answer: the key must be the
        // certificate's own. A certificate that hashes to the pinned
        // fingerprint cannot be changed, and the only point in it whose
        // private key anyone holds is its owner's.
        if self.certificate.is_empty()
            || self.certificate.len() > MAX_CERTIFICATE
            || self.key[0] != 0x04
            || !self
                .certificate
                .windows(self.key.len())
                .any(|window| window == self.key.as_slice())
        {
            return Err(Error::Invalid);
        }
        VerifyingKey::from_sec1_bytes(&self.key).map_err(|_| Error::Invalid)?;
        Ok(())
    }
}

/// How a host was added: what runs there, how to log in, what pins it,
/// and where it was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostIntro {
    pub kind: HostKind,
    pub name: String,
    /// The account members log in as.
    pub user: String,
    /// The certificate authority of a computer or Sheprd host; none for a
    /// plain host.
    pub ca: Option<Ca>,
    /// Where it was when added.
    pub record: SelfRecord,
    /// The record signed by `ca`'s key; none for a plain host, whose
    /// record the adding member vouches for.
    pub record_signature: Option<RawSignature>,
    /// How the adding member checked the record's SSH host keys.
    pub verified: Verified,
}

impl HostIntro {
    /// Its canonical bytes, as a `HostAdd` carries it.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        self.write(&mut writer)?;
        Ok(writer.into_bytes())
    }

    /// Reads an intro from [`HostIntro::to_bytes`], strictly.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let intro = Self::read(&mut reader)?;
        reader.finish()?;
        Ok(intro)
    }

    /// The host's id: its CA's fingerprint, or for a plain host the hash
    /// of its first SSH host key.
    pub fn id(&self) -> Result<HostId> {
        match &self.ca {
            Some(ca) => Ok(HostId(ca.fingerprint())),
            None => {
                let first = self.record.ssh_host_keys.first().ok_or(Error::Invalid)?;
                Ok(HostId::plain(first))
            }
        }
    }

    /// Whether `record` is this host's word: signed by its CA when it has
    /// one. A plain host's record is vouched for by the member that read
    /// it, so `signature` must be absent.
    pub fn vouches(&self, record: &SelfRecord, signature: Option<&RawSignature>) -> bool {
        match (&self.ca, signature) {
            (Some(ca), Some(signature)) => {
                let (Ok(key), Ok(signature), Some(message)) = (
                    VerifyingKey::from_sec1_bytes(&ca.key),
                    Signature::from_slice(signature),
                    record.message(),
                ) else {
                    return false;
                };
                key.verify(&message, &signature).is_ok()
            }
            (None, None) => true,
            _ => false,
        }
    }

    pub(super) fn check(&self) -> Result<()> {
        name(&self.name, MAX_NAME)?;
        name(&self.user, MAX_USER)?;
        self.record.check()?;
        match (self.kind, &self.ca) {
            (HostKind::Member | HostKind::Sheprd, Some(ca)) => ca.check()?,
            (HostKind::Agent, None) => {}
            _ => return Err(Error::Invalid),
        }
        if self.record.host != self.id()? {
            return Err(Error::Invalid);
        }
        if !self.vouches(&self.record, self.record_signature.as_ref()) {
            return Err(Error::SelfRecord);
        }
        Ok(())
    }

    pub(super) fn write(&self, writer: &mut Writer) -> Result<()> {
        self.check()?;
        self.kind.write(writer);
        writer.text(&self.name)?;
        writer.text(&self.user)?;
        writer.option(self.ca.as_ref(), |writer, ca| {
            writer.bytes(&ca.certificate)?;
            writer.raw(&ca.key);
            Ok(())
        })?;
        self.record.write(writer)?;
        writer.option(self.record_signature.as_ref(), |writer, signature| {
            writer.raw(signature);
            Ok(())
        })?;
        self.verified.write(writer);
        Ok(())
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Result<Self> {
        let intro = Self {
            kind: HostKind::read(reader)?,
            name: reader.text(MAX_NAME)?,
            user: reader.text(MAX_USER)?,
            ca: reader.option(|reader| {
                Ok(Ca {
                    certificate: reader.bytes(MAX_CERTIFICATE)?,
                    key: reader.array()?,
                })
            })?,
            record: SelfRecord::read(reader)?,
            record_signature: reader.option(Reader::array)?,
            verified: Verified::read(reader)?,
        };
        intro.check()?;
        Ok(intro)
    }
}

/// Where a host is, in its own words (or, for a plain host, as a member
/// read it over a strictly checked SSH session). `rev` only goes up.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelfRecord {
    pub host: HostId,
    pub rev: u64,
    /// Its relay route, base64url.
    pub route: String,
    /// Its relays, nearest first.
    pub relays: Vec<String>,
    /// Its tunnel identity: X25519 then ML-KEM-768, base64url.
    pub identity: String,
    /// Addresses a direct tunnel may try, `HOST:PORT`.
    pub direct: Vec<String>,
    /// Its SSH host keys, `TYPE BASE64`, strongest first.
    pub ssh_host_keys: Vec<String>,
    pub signed_ms: u64,
}

impl SelfRecord {
    /// What the host's CA signs.
    #[must_use]
    pub fn message(&self) -> Option<Vec<u8>> {
        let mut writer = Writer::new();
        writer.raw(DOMAIN);
        writer.raw(SELF_RECORD);
        self.write(&mut writer).ok()?;
        Some(writer.into_bytes())
    }

    pub(super) fn check(&self) -> Result<()> {
        field(&self.route, MAX_FIELD)?;
        field(&self.identity, MAX_FIELD)?;
        if self.relays.is_empty()
            || self.relays.len() > MAX_RELAYS
            || self.direct.len() > MAX_DIRECT
            || self.ssh_host_keys.is_empty()
            || self.ssh_host_keys.len() > MAX_HOST_KEYS
        {
            return Err(Error::Invalid);
        }
        self.relays
            .iter()
            .try_for_each(|relay| field(relay, MAX_FIELD))?;
        self.direct
            .iter()
            .try_for_each(|address| field(address, MAX_ADDRESS))?;
        self.ssh_host_keys
            .iter()
            .try_for_each(|key| ssh_key(key, MAX_HOST_KEY))
    }

    pub(super) fn write(&self, writer: &mut Writer) -> Result<()> {
        self.check()?;
        writer.raw(&self.host.0);
        writer.u64(self.rev);
        writer.text(&self.route)?;
        writer.texts(&self.relays)?;
        writer.text(&self.identity)?;
        writer.texts(&self.direct)?;
        writer.texts(&self.ssh_host_keys)?;
        writer.u64(self.signed_ms);
        Ok(())
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Result<Self> {
        let record = Self {
            host: HostId(reader.array()?),
            rev: reader.u64()?,
            route: reader.text(MAX_FIELD)?,
            relays: reader.texts(MAX_RELAYS, MAX_FIELD)?,
            identity: reader.text(MAX_FIELD)?,
            direct: reader.texts(MAX_DIRECT, MAX_ADDRESS)?,
            ssh_host_keys: reader.texts(MAX_HOST_KEYS, MAX_HOST_KEY)?,
            signed_ms: reader.u64()?,
        };
        record.check()?;
        Ok(record)
    }
}

/// A member admitted by another, after a person's approval there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Admit {
    pub bundle: Bundle,
    pub method: Method,
    /// The joiner's own signature over [`Bundle::join_message`].
    pub joiner_signature: RawSignature,
}

/// A member removed, with what it introduced that goes with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Remove {
    pub member: MemberId,
    pub also_members: Vec<MemberId>,
    pub also_hosts: Vec<HostId>,
    pub reason: Reason,
}

impl Remove {
    /// Every member this removal removes.
    pub fn targets(&self) -> impl Iterator<Item = &MemberId> {
        std::iter::once(&self.member).chain(&self.also_members)
    }
}

/// A computer member allowed on a host, or not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Access {
    pub host: HostId,
    pub member: MemberId,
    pub allow: bool,
    /// One above the highest counted `Access` for this host and member it
    /// descends from, so an agent that only sees the latest can merge by
    /// it, and no one operation can put the register out of reach.
    pub rev: u64,
}

/// A host's newer self-record, carried by a member.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostSeen {
    pub record: SelfRecord,
    /// The host CA's signature; none for a plain host, which the signing
    /// computer vouches for.
    pub signature: Option<RawSignature>,
}

/// A phone's own device key for one host's API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub host: HostId,
    /// P-256, SEC1.
    pub device_key: Vec<u8>,
}

/// What an operation says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Body {
    Found(Bundle),
    Admit(Admit),
    Remove(Remove),
    Rotate(Bundle),
    HostAdd(HostIntro),
    Access(Access),
    HostSeen(HostSeen),
    HostRemove(HostId),
    Link(Link),
    /// The hash of the state its history computes; the state itself
    /// travels beside it (`State::encode`).
    Checkpoint([u8; 32]),
}

impl Body {
    /// Whether it changes who is a member: the operations folded first.
    #[must_use]
    pub fn membership(&self) -> bool {
        matches!(
            self,
            Self::Found(_) | Self::Admit(_) | Self::Remove(_) | Self::Rotate(_)
        )
    }

    fn tag(&self) -> u8 {
        match self {
            Self::Found(_) => 1,
            Self::Admit(_) => 2,
            Self::Remove(_) => 3,
            Self::Rotate(_) => 4,
            Self::HostAdd(_) => 5,
            Self::Access(_) => 6,
            Self::HostSeen(_) => 7,
            Self::HostRemove(_) => 8,
            Self::Link(_) => 9,
            Self::Checkpoint(_) => 10,
        }
    }

    pub(super) fn write(&self, writer: &mut Writer) -> Result<()> {
        writer.u8(self.tag());
        match self {
            Self::Found(bundle) | Self::Rotate(bundle) => bundle.write(writer),
            Self::Admit(admit) => {
                admit.bundle.write(writer)?;
                admit.method.write(writer);
                writer.raw(&admit.joiner_signature);
                Ok(())
            }
            Self::Remove(remove) => {
                if remove.also_members.len() > MAX_ALSO_MEMBERS
                    || remove.also_hosts.len() > MAX_HOSTS
                {
                    return Err(Error::Bounds);
                }
                writer.raw(&remove.member.0);
                writer.count(remove.also_members.len())?;
                remove
                    .also_members
                    .iter()
                    .for_each(|member| writer.raw(&member.0));
                writer.count(remove.also_hosts.len())?;
                remove
                    .also_hosts
                    .iter()
                    .for_each(|host| writer.raw(&host.0));
                remove.reason.write(writer);
                Ok(())
            }
            Self::HostAdd(intro) => intro.write(writer),
            Self::Access(access) => {
                writer.raw(&access.host.0);
                writer.raw(&access.member.0);
                writer.bool(access.allow);
                writer.u64(access.rev);
                Ok(())
            }
            Self::HostSeen(seen) => {
                seen.record.write(writer)?;
                writer.option(seen.signature.as_ref(), |writer, signature| {
                    writer.raw(signature);
                    Ok(())
                })
            }
            Self::HostRemove(host) => {
                writer.raw(&host.0);
                Ok(())
            }
            Self::Link(link) => {
                device_key(&link.device_key)?;
                writer.raw(&link.host.0);
                writer.bytes(&link.device_key)
            }
            Self::Checkpoint(hash) => {
                writer.raw(hash);
                Ok(())
            }
        }
    }

    pub(super) fn read(reader: &mut Reader<'_>) -> Result<Self> {
        Ok(match reader.u8()? {
            1 => Self::Found(Bundle::read(reader)?),
            2 => Self::Admit(Admit {
                bundle: Bundle::read(reader)?,
                method: Method::read(reader)?,
                joiner_signature: reader.array()?,
            }),
            3 => {
                let member = MemberId(reader.array()?);
                let count = reader.count(MAX_ALSO_MEMBERS)?;
                let also_members = (0..count)
                    .map(|_| reader.array().map(MemberId))
                    .collect::<Result<_>>()?;
                let count = reader.count(MAX_HOSTS)?;
                let also_hosts = (0..count)
                    .map(|_| reader.array().map(HostId))
                    .collect::<Result<_>>()?;
                Self::Remove(Remove {
                    member,
                    also_members,
                    also_hosts,
                    reason: Reason::read(reader)?,
                })
            }
            4 => Self::Rotate(Bundle::read(reader)?),
            5 => Self::HostAdd(HostIntro::read(reader)?),
            6 => Self::Access(Access {
                host: HostId(reader.array()?),
                member: MemberId(reader.array()?),
                allow: reader.bool()?,
                rev: reader.u64()?,
            }),
            7 => Self::HostSeen(HostSeen {
                record: SelfRecord::read(reader)?,
                signature: reader.option(Reader::array)?,
            }),
            8 => Self::HostRemove(HostId(reader.array()?)),
            9 => {
                let host = HostId(reader.array()?);
                let device_key = reader.bytes(MAX_DEVICE_KEY)?;
                self::device_key(&device_key)?;
                Self::Link(Link { host, device_key })
            }
            10 => Self::Checkpoint(reader.array()?),
            _ => return Err(Error::Encoding),
        })
    }
}

fn point(key: &[u8]) -> Result<()> {
    VerifyingKey::from_sec1_bytes(key)
        .map(|_| ())
        .map_err(|_| Error::Invalid)
}

fn device_key(key: &[u8]) -> Result<()> {
    point(key)
}

/// A name a person reads: not blank, no control characters.
fn name(text: &str, max: usize) -> Result<()> {
    if text.trim().is_empty() || text.len() > max || text.chars().any(char::is_control) {
        return Err(Error::Invalid);
    }
    Ok(())
}

fn field(text: &str, max: usize) -> Result<()> {
    if text.is_empty()
        || text.len() > max
        || text.chars().any(|char| char.is_control() || char == ' ')
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

/// `TYPE BASE64`: two fields of printable ASCII, so it can go on an
/// `authorized_keys` or `known_hosts` line as it is.
fn ssh_key(text: &str, max: usize) -> Result<()> {
    let mut fields = text.split(' ');
    let (Some(kind), Some(blob), None) = (fields.next(), fields.next(), fields.next()) else {
        return Err(Error::Invalid);
    };
    if text.len() > max
        || kind.is_empty()
        || blob.is_empty()
        || !kind
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-@.".contains(&byte))
        || !blob
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte))
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

impl HostId {
    /// A plain host's id: the hash of its first SSH host key, `TYPE
    /// BASE64`, as its intro computes it. Sheprd finds an agent host among
    /// the machines it reaches by it.
    #[must_use]
    pub fn plain(ssh_host_key: &str) -> Self {
        Self(sha256(&[DOMAIN, HOST, ssh_host_key.as_bytes()]))
    }
}
