//! Canonical bytes: what a signature covers and what a hash names, in
//! one fixed layout so Swift and Rust agree byte for byte. Fixed-size
//! fields go as they are, big-endian; variable ones after their length as
//! a big-endian `u16`; lists after their count; optional fields after a
//! `0` or `1` byte. Reading is strict: a short field, a count over its
//! bound, a flag other than `0` or `1`, or bytes left over are errors.

use super::{Error, Result};

/// Writes canonical bytes.
#[derive(Debug, Default)]
pub(super) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }

    pub(super) fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }

    /// A count before a list; callers have checked it against its bound.
    pub(super) fn count(&mut self, count: usize) -> Result<()> {
        self.u16(u16::try_from(count).map_err(|_| Error::Bounds)?);
        Ok(())
    }

    /// Bytes after their length.
    pub(super) fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.count(bytes.len())?;
        self.raw(bytes);
        Ok(())
    }

    pub(super) fn text(&mut self, text: &str) -> Result<()> {
        self.bytes(text.as_bytes())
    }

    pub(super) fn texts(&mut self, texts: &[String]) -> Result<()> {
        self.count(texts.len())?;
        texts.iter().try_for_each(|text| self.text(text))
    }

    pub(super) fn option<T>(
        &mut self,
        value: Option<&T>,
        write: impl FnOnce(&mut Self, &T) -> Result<()>,
    ) -> Result<()> {
        match value {
            None => {
                self.u8(0);
                Ok(())
            }
            Some(value) => {
                self.u8(1);
                write(self, value)
            }
        }
    }
}

/// Reads canonical bytes.
#[derive(Debug)]
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// Whether every byte was read.
    pub(super) fn finish(&self) -> Result<()> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Error::Encoding)
        }
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len()
    }

    pub(super) fn raw(&mut self, length: usize) -> Result<&'a [u8]> {
        if self.bytes.len() < length {
            return Err(Error::Encoding);
        }
        let (head, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(head)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.raw(N)?.try_into().map_err(|_| Error::Encoding)
    }

    pub(super) fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub(super) fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Encoding),
        }
    }

    /// A count, refused over `max`.
    pub(super) fn count(&mut self, max: usize) -> Result<usize> {
        let count = usize::from(self.u16()?);
        if count > max {
            return Err(Error::Bounds);
        }
        Ok(count)
    }

    /// Bytes after their length, refused over `max`.
    pub(super) fn bytes(&mut self, max: usize) -> Result<Vec<u8>> {
        let length = self.count(max)?;
        Ok(self.raw(length)?.to_vec())
    }

    pub(super) fn text(&mut self, max: usize) -> Result<String> {
        String::from_utf8(self.bytes(max)?).map_err(|_| Error::Encoding)
    }

    pub(super) fn texts(&mut self, count: usize, max: usize) -> Result<Vec<String>> {
        let count = self.count(count)?;
        (0..count).map(|_| self.text(max)).collect()
    }

    pub(super) fn option<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<Option<T>> {
        if self.bool()? {
            read(self).map(Some)
        } else {
            Ok(None)
        }
    }
}
