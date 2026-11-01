//! The document's file: bounded reads, a cheap stamp to notice changes,
//! and atomic writes that refuse to overwrite a change they have not seen.
//! Every function blocks on the file system; run them off a UI thread.

use crate::{Error, MAX_BYTES, Result};
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

/// What changes when a file's contents do, as far as a poll can tell
/// without reading it: its length, modification time, and, on Unix, inode,
/// which an editor that saves by rename replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    inode: u64,
}

impl Stamp {
    fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        let inode = std::os::unix::fs::MetadataExt::ino(metadata);
        #[cfg(not(unix))]
        let inode = 0;
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            inode,
        }
    }
}

/// How a file spells what the document keeps plainly: its line breaks and
/// whether it starts with a byte-order mark. A document always holds `\n`
/// and no mark; a file keeps its own on every write, as comet's open files
/// keep their encoding and line endings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Format {
    /// Lines end in `\r\n`, as the file's first line break did.
    pub crlf: bool,
    /// The file starts with a UTF-8 byte-order mark.
    pub bom: bool,
}

const BOM: &str = "\u{feff}";

impl Format {
    /// `raw` file text as a document holds it, and how the file spelled it.
    pub fn decode(raw: &str) -> (String, Self) {
        let (bom, body) = match raw.strip_prefix(BOM) {
            Some(body) => (true, body),
            None => (false, raw),
        };
        let crlf = body.find('\n').is_some_and(|at| body[..at].ends_with('\r'));
        let text = if body.contains("\r\n") {
            body.replace("\r\n", "\n")
        } else {
            body.to_owned()
        };
        (text, Self { crlf, bom })
    }

    /// Document text as the file spells it.
    pub fn encode(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + usize::from(self.bom) * BOM.len());
        if self.bom {
            out.push_str(BOM);
        }
        if self.crlf {
            out.push_str(&text.replace('\n', "\r\n"));
        } else {
            out.push_str(text);
        }
        out
    }
}

/// The file's stamp, or `None` when it is gone.
pub fn stamp(path: &Path) -> Result<Option<Stamp>> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(Stamp::of(&metadata))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error("inspect", path, source)),
    }
}

/// Reads the file as UTF-8 text, as a document holds it, with its stamp
/// and format. Refuses a file that is not a regular file, is over
/// [`MAX_BYTES`], or is not UTF-8, each by its own variant, and reports a
/// missing file as [`Error::Missing`].
pub fn read(path: &Path) -> Result<(String, Stamp, Format)> {
    let (raw, stamp) = read_raw(path)?;
    let (text, format) = Format::decode(&raw);
    Ok((text, stamp, format))
}

fn read_raw(path: &Path) -> Result<(String, Stamp)> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Error::Missing { path: path.into() });
        }
        Err(source) => return Err(io_error("inspect", path, source)),
    };
    if !metadata.is_file() {
        return Err(Error::NotAFile { path: path.into() });
    }
    check_len(metadata.len())?;
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(Error::Missing { path: path.into() });
        }
        Err(source) => return Err(io_error("read", path, source)),
    };
    // The file may have grown between the check and the read.
    check_len(bytes.len() as u64)?;
    let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 { path: path.into() })?;
    Ok((text, Stamp::of(&metadata)))
}

static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What [`write_if_unchanged`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Written {
    /// The file now holds the new text.
    Wrote(Stamp),
    /// The file no longer held the expected text, so nothing was written;
    /// this is what it held, to merge before trying again.
    Changed(String),
    /// The file is gone; nothing was written, so a deleted or moved file is
    /// not brought back behind its owner's back.
    Missing,
}

/// Writes `text` to `path` atomically, but only while the file still holds
/// `expected`, the text last synced with it: another writer's change is
/// returned to be merged instead of being overwritten. A change landing
/// between this check and the rename is the one window left.
pub fn write_if_unchanged(
    path: &Path,
    expected: &str,
    text: &str,
    format: Format,
) -> Result<Written> {
    // This process's own writes take turns, so two of them (a document's
    // last write as it closes and one still in flight) never both pass the
    // check before either renames.
    let _turn = WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let current = match read(path) {
        Ok((current, ..)) => current,
        Err(Error::Missing { .. }) => return Ok(Written::Missing),
        Err(error) => return Err(error),
    };
    if current != expected {
        return Ok(Written::Changed(current));
    }
    write_unlocked(path, &format.encode(text)).map(Written::Wrote)
}

/// Writes `text` to `path` through a temporary file in the same folder and
/// a rename, so a reader sees the old text or the new one, never half. An
/// existing file keeps its permissions.
pub fn write(path: &Path, text: &str, format: Format) -> Result<Stamp> {
    let _turn = WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    write_unlocked(path, &format.encode(text))
}

/// [`write`] when the file is gone: brings it back, but only while it is
/// still gone; a file that reappeared meanwhile is left for the caller to
/// merge.
pub fn recreate(path: &Path, expected: &str, text: &str, format: Format) -> Result<Written> {
    let _turn = WRITES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let encoded = format.encode(text);
    match read(path) {
        Err(Error::Missing { .. }) => write_unlocked(path, &encoded).map(Written::Wrote),
        Ok((current, ..)) if current == expected => {
            write_unlocked(path, &encoded).map(Written::Wrote)
        }
        Ok((current, ..)) => Ok(Written::Changed(current)),
        Err(error) => Err(error),
    }
}

fn write_unlocked(path: &Path, text: &str) -> Result<Stamp> {
    check_len(text.len() as u64)?;
    let temporary = temporary_path(path);
    let result = write_temporary(path, &temporary, text);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_temporary(path: &Path, temporary: &Path, text: &str) -> Result<Stamp> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|source| io_error("create", temporary, source))?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| io_error("write", temporary, source))?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(temporary, metadata.permissions())
            .map_err(|source| io_error("set the permissions of", temporary, source))?;
    }
    drop(file);
    fs::rename(temporary, path).map_err(|source| io_error("replace", path, source))?;
    let metadata = fs::metadata(path).map_err(|source| io_error("inspect", path, source))?;
    Ok(Stamp::of(&metadata))
}

/// A name beside `path` no other write of this process uses, hidden on
/// Unix, so a crash leaves at most a dotfile behind.
fn temporary_path(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(
        ".{name}.herdr-doc-{}-{unique}.tmp",
        std::process::id()
    ))
}

fn check_len(len: u64) -> Result<()> {
    if len > MAX_BYTES as u64 {
        return Err(Error::TooLarge {
            len,
            limit: MAX_BYTES,
        });
    }
    Ok(())
}

fn io_error(operation: &'static str, path: &Path, source: io::Error) -> Error {
    Error::Io {
        operation,
        path: path.into(),
        source,
    }
}
