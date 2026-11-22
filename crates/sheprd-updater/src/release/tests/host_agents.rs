//! `sheprd-relay-agent` (schema 6), which plain hosts run: what only its
//! family checks.

use super::{
    packed::{VERSION, archive, manifest, unpack},
    *,
};
use crate::release::{host_agent::AGENTS, host_chat::CHATS};

#[test]
fn the_archive_is_named_for_the_release() {
    assert_eq!(
        AGENTS.archive_name(VERSION),
        "sheprd-relay-agent-20261108.3.tar.gz"
    );
}

#[test]
fn every_target_is_a_static_linux_or_a_mac_build() {
    for target in AGENT_TARGETS {
        assert!(
            target.ends_with("-unknown-linux-musl") || target.ends_with("-apple-darwin"),
            "{target}"
        );
    }
    // Each target once.
    for (index, target) in AGENT_TARGETS.iter().enumerate() {
        assert!(!AGENT_TARGETS[..index].contains(target));
    }
}

#[test]
fn no_other_family_shares_its_schema_or_manifest() {
    assert_ne!(AGENTS.schema, CHATS.schema);
    assert_ne!(AGENTS.schema, host::SCHEMA);
    assert_ne!(AGENTS.manifest.0, CHATS.manifest.0);
    assert_ne!(AGENTS.manifest.0, host::MANIFEST.0);
    for format in [Format::Archive, Format::Deb, Format::Installer] {
        assert_ne!(format.schema(), AGENTS.schema);
        assert_ne!(format.manifest().0, AGENTS.manifest.0);
    }
}

#[test]
fn a_chat_build_never_answers_for_an_agent() -> anyhow::Result<()> {
    // The chat archive, signed by the release's key, holds `sheprd-acp`
    // for a target the agent also ships: read as the agent's, it is refused.
    let build = b"static sheprd-acp";
    let target = HOST_TARGETS[0];
    let chats = archive(&CHATS, &manifest(&CHATS, build), 42, build);
    let (result, dir) = unpack(&AGENTS, &chats, target);
    assert!(
        matches!(result, Err(Error::HostArchiveLayout)),
        "{result:?}"
    );
    assert!(!dir.path().join(AGENTS.program).exists());
    // Nor does an agent manifest whose names are another program's.
    let mut renamed = manifest(&AGENTS, build);
    for asset in &mut renamed.assets {
        asset.name = format!("sheprd-acp-{VERSION}-{}", asset.target);
    }
    let (result, _) = unpack(&AGENTS, &archive(&AGENTS, &renamed, 42, build), target);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn a_manifest_listing_more_than_its_targets_is_refused() {
    let build = b"static build";
    let mut crowded = manifest(&AGENTS, build);
    let extra = crowded.assets[0].clone();
    crowded.assets.push(extra);
    let (result, _) = unpack(
        &AGENTS,
        &archive(&AGENTS, &crowded, 42, build),
        AGENT_TARGETS[0],
    );
    assert!(
        matches!(result, Err(Error::ManifestAssetCount)),
        "{result:?}"
    );
}

#[test]
fn a_manifest_for_another_release_is_refused() {
    let build = b"static build";
    let mut older = manifest(&AGENTS, build);
    older.version = "20261108.2".into();
    for asset in &mut older.assets {
        asset.name = format!("sheprd-relay-agent-20261108.2-{}", asset.target);
    }
    let (result, dir) = unpack(
        &AGENTS,
        &archive(&AGENTS, &older, 42, build),
        AGENT_TARGETS[2],
    );
    assert!(result.is_err());
    assert!(!dir.path().join(AGENTS.program).exists());
}

#[test]
fn the_release_script_packs_the_same_family() {
    // The release's packer and this reader must agree on every name.
    let script = include_str!("../../../../../scripts/update-manifest.py");
    for target in AGENT_TARGETS {
        assert!(script.contains(&format!("\"{target}\"")), "{target}");
    }
    assert!(script.contains(&format!(
        "HOST_AGENT_MANIFEST = (\"{}\", \"{}\")",
        AGENTS.manifest.0, AGENTS.manifest.1
    )));
    assert!(script.contains(&format!(
        "\"host-agents\": ({}, HOST_AGENT_MANIFEST, \"{}\", host_agent_archive)",
        AGENTS.schema, AGENTS.program
    )));
    assert!(script.contains("return f\"sheprd-relay-agent-{release}.tar.gz\""));
}
