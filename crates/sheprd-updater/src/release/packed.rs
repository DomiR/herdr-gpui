//! Host builds that travel in one archive per release, with their own
//! signed manifest packed first, so a release stays under the asset limit
//! installed updaters read: the host builds' `sheprd-acp` (`host_chat.rs`,
//! schema 5) and `sheprd-relay-agent` (`host_agent.rs`, schema 6). The
//! manifest's signature is checked before any build in the archive is
//! read, and the build against the manifest before it leaves this
//! computer.

use super::*;
use flate2::read::GzDecoder;
use std::{cell::Cell, io, rc::Rc};

/// What tar adds around each entry at most: its header, the padding of its
/// data to a block, and a long name's extra header and block.
const ENTRY_OVERHEAD: u64 = 4 * 512;

/// One family of packed host builds.
#[derive(Debug)]
pub(super) struct Pack {
    /// The schema its manifest carries, which no other family uses.
    pub(super) schema: u32,
    /// The signed manifest and its signature, the archive's first two entries.
    pub(super) manifest: (&'static str, &'static str),
    /// The program each target's build is, at `TARGET/PROGRAM` in the
    /// archive and listed as `PROGRAM-VERSION-TARGET`.
    pub(super) program: &'static str,
    /// What follows `PROGRAM-VERSION` in the archive's name.
    pub(super) suffix: &'static str,
    /// The targets it publishes builds for.
    pub(super) targets: &'static [&'static str],
}

impl Pack {
    /// The release asset holding every target's build.
    pub(super) fn archive_name(&self, version: &str) -> String {
        format!("{}-{version}{}", self.program, self.suffix)
    }

    pub(super) fn accepts(&self, version: &str, target: &str, asset: &str) -> bool {
        self.targets.contains(&target) && asset == format!("{}-{version}-{target}", self.program)
    }

    /// Downloads release `version`'s build for `target` into `destination`,
    /// checked against the signed manifest in its archive by `key`. On
    /// error the caller removes `destination`. Blocking.
    pub(super) fn download(
        &self,
        version: &str,
        key: &str,
        target: &str,
        destination: &Path,
        cancel: &AtomicBool,
    ) -> Result<()> {
        if !self.targets.contains(&target) {
            return Err(Error::UnsupportedTarget);
        }
        let response = request(
            &release_url(version, &self.archive_name(version)),
            RequestProfile::Archive,
            cancel,
        )?;
        unpack(
            self,
            response.into_body().into_reader(),
            version,
            key,
            target,
            destination,
            cancel,
        )
    }
}

/// Reads the archive from `reader`, checks its manifest against `key_hex`
/// and `version`, and writes `target`'s build to `destination`, which must
/// not exist yet and is created private.
pub(super) fn unpack(
    pack: &Pack,
    reader: impl Read,
    version: &str,
    key_hex: &str,
    target: &str,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    if !pack.targets.contains(&target) {
        return Err(Error::UnsupportedTarget);
    }
    // Until the manifest is checked, only it and its signature may expand.
    let limit = Rc::new(Cell::new(MANIFEST_LIMIT as u64 + 64 + 2 * ENTRY_OVERHEAD));
    let stop = Rc::new(Cell::new(None));
    let guarded = Guarded {
        cancel,
        inner: GzDecoder::new(reader.take(ARCHIVE_LIMIT)),
        limit: limit.clone(),
        read: 0,
        stop: stop.clone(),
    };
    let result = read_archive(pack, guarded, version, key_hex, target, destination);
    // Whatever a reader made of the guard's refusal, say why it stopped.
    match (result, stop.get()) {
        (Err(_), Some(Stop::Cancelled)) => Err(Error::Cancelled),
        (Err(_), Some(Stop::Expanded)) => Err(Error::HostArchiveExpanded),
        (result, _) => result,
    }
}

/// Reads `reader`'s entries, raising its expansion limit once the signed
/// manifest says how large its builds are.
fn read_archive<R: Read>(
    pack: &Pack,
    reader: Guarded<'_, R>,
    version: &str,
    key_hex: &str,
    target: &str,
    destination: &Path,
) -> Result<()> {
    let (cancel, limit) = (reader.cancel, reader.limit.clone());
    let mut archive = tar::Archive::new(reader);
    let mut entries = archive.entries().map_err(Error::ReadArchive)?;
    let mut next = |name: &str, limit: usize| -> Result<Vec<u8>> {
        let entry = entries
            .next()
            .ok_or(Error::HostArchiveLayout)?
            .map_err(Error::ReadArchive)?;
        if entry.path().map_err(Error::ReadArchive)? != Path::new(name) {
            return Err(Error::HostArchiveLayout);
        }
        read_bounded(entry, limit, cancel)
    };
    let manifest = next(pack.manifest.0, MANIFEST_LIMIT)?;
    let signature = next(pack.manifest.1, 64)?;
    let manifest = verify_manifest(&manifest, &signature, key_hex, version)?;
    // A manifest signed for another family must not answer for this one.
    if manifest.schema != pack.schema {
        return Err(Error::ManifestSchema);
    }
    // Past the manifest, the archive holds the builds it signs and nothing
    // else: unsigned data there is refused rather than read through.
    let builds = manifest.assets.iter().fold(0u64, |total, asset| {
        total
            .saturating_add(asset.size)
            .saturating_add(ENTRY_OVERHEAD)
    });
    limit.set(limit.get().saturating_add(builds));
    let asset = manifest
        .assets
        .into_iter()
        .find(|asset| asset.target == target && pack.accepts(version, target, &asset.name))
        .ok_or(Error::MissingPlatformAsset)?;
    let wanted = Path::new(target).join(pack.program);
    for entry in entries {
        cancelled(cancel)?;
        let entry = entry.map_err(Error::ReadArchive)?;
        if entry.path().map_err(Error::ReadArchive)? != wanted {
            continue;
        }
        let mut file = create_archive(destination)?;
        copy_archive(entry, &mut file, &asset, cancel, |_, _| {})?;
        file.sync_all().map_err(Error::SyncArchive)?;
        return cancelled(cancel);
    }
    Err(Error::MissingPlatformAsset)
}

/// Why [`Guarded`] stopped reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Cancelled,
    Expanded,
}

/// The expanded archive, read no further than `limit` and stopping once
/// cancelled, including through entries tar skips without handing out.
struct Guarded<'a, R> {
    cancel: &'a AtomicBool,
    inner: R,
    limit: Rc<Cell<u64>>,
    read: u64,
    stop: Rc<Cell<Option<Stop>>>,
}

impl<R: Read> Read for Guarded<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(self.refuse(Stop::Cancelled));
        }
        let left = self.limit.get().saturating_sub(self.read);
        // One byte past the limit tells a full archive from a longer one.
        let room = usize::try_from(left.saturating_add(1)).unwrap_or(usize::MAX);
        let take = buffer.len().min(room);
        let count = self.inner.read(&mut buffer[..take])?;
        self.read += count as u64;
        if self.read > self.limit.get() {
            return Err(self.refuse(Stop::Expanded));
        }
        Ok(count)
    }
}

impl<R> Guarded<'_, R> {
    fn refuse(&self, stop: Stop) -> io::Error {
        self.stop.set(Some(stop));
        io::Error::other(match stop {
            Stop::Cancelled => "cancelled",
            Stop::Expanded => "archive expands past its signed builds",
        })
    }
}
