//! SSH between members through the relay: allowed members reach the
//! stand-in SSH server; an excluded or removed one does not, and a live
//! session ends when the fleet stops allowing it.

use super::*;
use crate::desktop::{connect, record::Record};

/// The host name `relay-connect` dials for `to`, as `from` keeps it.
fn host_name(to: &Node) -> String {
    let info = to.relays.info().unwrap();
    let record = Record {
        computer: String::new(),
        identity: info.identity,
        relays: info.relays,
        route: info.route,
        fleet: true,
        host: None,
        opening: true,
    };
    sheprd_client::relay_host::host_name(&record.id().unwrap())
}

async fn banner(from: &Node, to: &Node) -> std::io::Result<String> {
    let (_connection, mut tunnel) = open(from, to).await?;
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await?;
    Ok(String::from_utf8_lossy(&greeting).into_owned())
}

async fn open(
    from: &Node,
    to: &Node,
) -> std::io::Result<(quinn::Connection, sheprd_tunnel::stream::Tunnel)> {
    connect::open(from.dir.path(), &host_name(to))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))
}

/// Opens `from`'s SSH session to `to` once `to`'s daemon names `from`'s
/// dial key on the relay, which it does just after applying the grant.
async fn opened(from: &Node, to: &Node) -> (quinn::Connection, sheprd_tunnel::stream::Tunnel) {
    for _ in 0..100 {
        if let Ok(opened) = open(from, to).await {
            return opened;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("never let through");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_member_reaches_another_over_ssh_through_the_relay() {
    let (a, b, _c) = three().await;
    until(10, || b.lines().len() == 2).await;
    until(10, || {
        b.dir
            .path()
            .join("desktop/hosts")
            .read_dir()
            .is_ok_and(|entries| entries.count() == 2)
    })
    .await;
    let (_connection, mut tunnel) = opened(&b, &a).await;
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    assert_eq!(&greeting, b"SSH-2.0-test\r\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_excluded_member_gets_no_line_record_or_session() {
    let (a, _b, c) = three().await;
    until(10, || a.lines().len() == 2).await;
    let host = a
        .view()
        .hosts
        .into_iter()
        .find(|host| host.added_by == a.me())
        .unwrap();
    a.ask(Ask::Access {
        host: host.id,
        member: c.me(),
        allow: false,
    });
    until(10, || a.lines().len() == 1).await;
    assert!(!a.lines().iter().any(|line| line.ends_with(&c.me()[..16])));
    // c hears of it through sync, as Sheprd over SSH would carry it.
    c.fleet.receive(a.ops(), now_ms()).unwrap();
    until(10, || {
        Record::load(
            c.dir.path(),
            sheprd_client::relay_host::host_id(&host_name(&a)).unwrap(),
        )
        .is_err()
    })
    .await;
    assert!(banner(&c, &a).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_member_ends_its_live_session() {
    let (a, b, _c) = three().await;
    until(10, || b.lines().len() == 2 && a.lines().len() == 2).await;
    until(10, || {
        b.dir
            .path()
            .join("desktop/hosts")
            .read_dir()
            .is_ok_and(|entries| entries.count() == 2)
    })
    .await;
    let (_connection, mut tunnel) = opened(&b, &a).await;
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    a.ask(Ask::Remove {
        member: b.me(),
        also_members: Vec::new(),
        also_hosts: Vec::new(),
        reason: crate::control::fleet::Reason::Lost,
    });
    let mut rest = Vec::new();
    let ended =
        tokio::time::timeout(Duration::from_secs(10), tunnel.io.read_to_end(&mut rest)).await;
    assert!(ended.is_ok(), "the session outlived the removal");
    until(10, || a.lines().len() == 1).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_scrub_ends_every_other_members_session_at_once() {
    let (a, b, _c) = three().await;
    until(10, || b.lines().len() == 2 && a.lines().len() == 2).await;
    until(10, || {
        b.dir
            .path()
            .join("desktop/hosts")
            .read_dir()
            .is_ok_and(|entries| entries.count() == 2)
    })
    .await;
    let (_connection, mut tunnel) = opened(&b, &a).await;
    let mut greeting = [0; 14];
    tunnel.io.read_exact(&mut greeting).await.unwrap();
    // Another session asked for the scrub: b's keeps no grace.
    a.ask(Ask::Scrub { session: Some(1) });
    let mut rest = Vec::new();
    let ended = tokio::time::timeout(SCRUB_GRACE / 4, tunnel.io.read_to_end(&mut rest)).await;
    assert!(ended.is_ok(), "the session outlived the scrub");
}
