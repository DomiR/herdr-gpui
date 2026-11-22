//! A signed operation: who signed it, what it had seen, and what it says.

use super::{
    Body, DOMAIN, Error, FleetId, MAX_OP, MAX_PARENTS, MemberId, OpId, Result,
    body::RawSignature,
    sha256,
    wire::{Reader, Writer},
};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey, signature::Signer, signature::Verifier};

const VERSION: u8 = 1;
const SIGNATURE_LEN: usize = 64;

/// An operation before it is signed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unsigned {
    /// The fleet; zero in the `Found` that makes it.
    pub fleet: FleetId,
    pub signer: MemberId,
    /// Above every earlier operation by the same signer.
    pub seq: u64,
    /// The signer's clock: checked for skew on arrival and shown, never
    /// used to order.
    pub issued_ms: u64,
    /// The heads the signer had applied; none only in `Found`, and the
    /// checkpoint a log was pruned to counts as present.
    pub parents: Vec<OpId>,
    pub body: Body,
}

impl Unsigned {
    /// The bytes its signature covers and its id hashes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.check_shape()?;
        let mut writer = Writer::new();
        writer.raw(DOMAIN);
        writer.u8(VERSION);
        writer.raw(&self.fleet.0);
        writer.raw(&self.signer.0);
        writer.u64(self.seq);
        writer.u64(self.issued_ms);
        writer.u8(u8::try_from(self.parents.len()).map_err(|_| Error::Bounds)?);
        self.parents.iter().for_each(|parent| writer.raw(&parent.0));
        self.body.write(&mut writer)?;
        let bytes = writer.into_bytes();
        if bytes.len() + SIGNATURE_LEN > MAX_OP {
            return Err(Error::Bounds);
        }
        Ok(bytes)
    }

    /// Signs it with `key`, which must be `signer`'s.
    pub fn sign(self, key: &SigningKey) -> Result<Op> {
        let bytes = self.encode()?;
        let signature: Signature = key.sign(&bytes);
        let signature: RawSignature = signature
            .to_bytes()
            .as_slice()
            .try_into()
            .map_err(|_| Error::Signature)?;
        Ok(Op::from_parts(self, signature, bytes))
    }

    fn check_shape(&self) -> Result<()> {
        let found = matches!(self.body, Body::Found(_));
        if self.parents.len() > MAX_PARENTS {
            return Err(Error::Bounds);
        }
        let mut sorted = self.parents.clone();
        sorted.sort_unstable();
        sorted.dedup();
        // Parents are a set, written in order, so one operation has one
        // encoding and one id.
        if sorted != self.parents
            || found != self.parents.is_empty()
            || found != (self.fleet == FleetId([0; 32]))
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

/// A signed operation, with its encoding and id kept.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Op {
    unsigned: Unsigned,
    signature: RawSignature,
    /// The signed bytes, without the signature.
    signed: Vec<u8>,
    id: OpId,
}

impl Op {
    fn from_parts(unsigned: Unsigned, signature: RawSignature, signed: Vec<u8>) -> Self {
        let id = OpId(sha256(&[&signed]));
        Self {
            unsigned,
            signature,
            signed,
            id,
        }
    }

    /// Reads an operation, checking its shape and bounds; its signature is
    /// checked against its signer's key once that key is known
    /// ([`Op::verify`]).
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_OP || bytes.len() < SIGNATURE_LEN {
            return Err(Error::Bounds);
        }
        let (signed, signature) = bytes.split_at(bytes.len() - SIGNATURE_LEN);
        let mut reader = Reader::new(signed);
        if reader.raw(DOMAIN.len())? != DOMAIN || reader.u8()? != VERSION {
            return Err(Error::Encoding);
        }
        let fleet = FleetId(reader.array()?);
        let signer = MemberId(reader.array()?);
        let seq = reader.u64()?;
        let issued_ms = reader.u64()?;
        let count = usize::from(reader.u8()?);
        if count > MAX_PARENTS || reader.remaining() < count * 32 {
            return Err(Error::Bounds);
        }
        let parents = (0..count)
            .map(|_| reader.array().map(OpId))
            .collect::<Result<_>>()?;
        let body = Body::read(&mut reader)?;
        reader.finish()?;
        let unsigned = Unsigned {
            fleet,
            signer,
            seq,
            issued_ms,
            parents,
            body,
        };
        // Encoding again must give the same bytes: one operation, one
        // encoding.
        if unsigned.encode()? != signed {
            return Err(Error::Encoding);
        }
        let signature = signature.try_into().map_err(|_| Error::Encoding)?;
        Ok(Self::from_parts(unsigned, signature, signed.to_vec()))
    }

    /// The bytes that travel and are stored: the signed bytes, then the
    /// signature.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        [self.signed.as_slice(), &self.signature].concat()
    }

    #[must_use]
    pub fn id(&self) -> OpId {
        self.id
    }

    #[must_use]
    pub fn unsigned(&self) -> &Unsigned {
        &self.unsigned
    }

    #[must_use]
    pub fn signer(&self) -> MemberId {
        self.unsigned.signer
    }

    #[must_use]
    pub fn body(&self) -> &Body {
        &self.unsigned.body
    }

    #[must_use]
    pub fn parents(&self) -> &[OpId] {
        &self.unsigned.parents
    }

    #[must_use]
    pub fn signature(&self) -> &RawSignature {
        &self.signature
    }

    /// Whether `key` signed it.
    pub fn verify(&self, key: &VerifyingKey) -> Result<()> {
        let signature = Signature::from_slice(&self.signature).map_err(|_| Error::Signature)?;
        key.verify(&self.signed, &signature)
            .map_err(|_| Error::Signature)
    }
}
