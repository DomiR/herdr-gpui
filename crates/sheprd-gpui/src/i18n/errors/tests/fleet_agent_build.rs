use super::*;
use crate::fleet::agent::build::Error;
use std::{io, path::PathBuf};

#[test]
fn every_agent_build_variant_has_a_message() {
    assert_localized(&[
        Error::Cache {
            path: PathBuf::from("/state/remote/host-builds/20261120.1"),
            source: io::Error::other("disk full"),
        },
        Error::Download {
            target: "aarch64-unknown-linux-musl".into(),
            source: sheprd_updater::Error::MissingPlatformAsset,
        },
        Error::Host(crate::host_build::Error::Checksum),
        Error::NoBuild {
            platform: "Linux x86_64".into(),
        },
        Error::NoStateDir,
        Error::Unsupported {
            platform: "FreeBSD amd64".into(),
        },
    ]);
}
