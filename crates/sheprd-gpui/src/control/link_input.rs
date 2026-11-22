//! The pairing link `sheprd remote add-link` reads from standard input
//! (`sheprd_remote_control::control::pairing_link`). It is never an
//! argument, so it stays out of shell history and the process list, and
//! nothing here prints or logs it.

mod error;

pub(crate) use error::Error;

use sheprd_remote_control::{app::pairing::LinkText, control::pairing_link};
use std::io::Read;

/// The most read: a link and the white space a chat added around it.
pub(crate) const MAX_INPUT: usize = 2 * pairing_link::MAX_LEN;

/// The link in `input`, checked: at most [`MAX_INPUT`] bytes of UTF-8 that
/// read as a pairing link. White space a chat inserted is dropped.
pub(crate) fn read(input: impl Read) -> Result<LinkText, Error> {
    let mut bytes = Vec::new();
    input
        .take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() > MAX_INPUT {
        return Err(Error::TooLong);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::Text)?;
    if text.trim().is_empty() {
        return Err(Error::Empty);
    }
    text.parse::<pairing_link::PairingLink>()?;
    let link: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    Ok(LinkText(link))
}

#[cfg(test)]
mod tests;
