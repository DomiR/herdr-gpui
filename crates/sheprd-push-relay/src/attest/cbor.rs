//! The subset of CBOR (RFC 8949) an App Attest object uses: unsigned
//! integers, byte and text strings, arrays, and maps, all of definite
//! length. Anything else is refused, as are deep nesting, long containers,
//! and trailing bytes, so a hostile body costs little to reject.

use super::Error;

/// Deeper than an attestation nests (map, map, array, bytes).
const MAX_DEPTH: usize = 4;
/// More entries than any container in an attestation holds.
const MAX_ITEMS: u64 = 16;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Value<'a> {
    Array(Vec<Value<'a>>),
    Bytes(&'a [u8]),
    Map(Vec<(Value<'a>, Value<'a>)>),
    Text(&'a str),
    Unsigned(u64),
}

impl<'a> Value<'a> {
    /// The value under the text key `name`, when this is a map.
    pub(super) fn get(&self, name: &str) -> Option<&Value<'a>> {
        let Self::Map(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find_map(|(key, value)| (*key == Value::Text(name)).then_some(value))
    }

    pub(super) fn bytes(&self) -> Option<&'a [u8]> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }
}

/// The one value `input` holds, with nothing after it.
pub(super) fn parse(input: &[u8]) -> Result<Value<'_>, Error> {
    let mut reader = Reader { input, at: 0 };
    let value = reader.value(0)?;
    if reader.at == input.len() {
        Ok(value)
    } else {
        Err(Error::Cbor)
    }
}

struct Reader<'a> {
    input: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(count).ok_or(Error::Cbor)?;
        let taken = self.input.get(self.at..end).ok_or(Error::Cbor)?;
        self.at = end;
        Ok(taken)
    }

    /// The major type and its argument. Indefinite lengths (31) and the
    /// reserved values (28 to 30) are refused.
    fn head(&mut self) -> Result<(u8, u64), Error> {
        let [first] = self.take(1)? else {
            return Err(Error::Cbor);
        };
        let argument = match first & 0x1f {
            small @ 0..=23 => u64::from(small),
            24 => u64::from(self.take(1)?[0]),
            25 => u64::from(u16::from_be_bytes(self.array()?)),
            26 => u64::from(u32::from_be_bytes(self.array()?)),
            27 => u64::from_be_bytes(self.array()?),
            _ => return Err(Error::Cbor),
        };
        Ok((first >> 5, argument))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?.try_into().map_err(|_| Error::Cbor)
    }

    fn length(argument: u64) -> Result<usize, Error> {
        usize::try_from(argument).map_err(|_| Error::Cbor)
    }

    fn value(&mut self, depth: usize) -> Result<Value<'a>, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Cbor);
        }
        let (major, argument) = self.head()?;
        match major {
            0 => Ok(Value::Unsigned(argument)),
            2 => Ok(Value::Bytes(self.take(Self::length(argument)?)?)),
            3 => std::str::from_utf8(self.take(Self::length(argument)?)?)
                .map(Value::Text)
                .map_err(|_| Error::Cbor),
            4 if argument <= MAX_ITEMS => (0..argument)
                .map(|_| self.value(depth + 1))
                .collect::<Result<_, _>>()
                .map(Value::Array),
            5 if argument <= MAX_ITEMS => (0..argument)
                .map(|_| Ok((self.value(depth + 1)?, self.value(depth + 1)?)))
                .collect::<Result<_, _>>()
                .map(Value::Map),
            _ => Err(Error::Cbor),
        }
    }
}
