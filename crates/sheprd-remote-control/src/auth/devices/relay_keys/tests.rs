use super::*;
use crate::auth::{NewDevice, PublicKey};
use p256::ecdsa::SigningKey;

fn key(seed: u8) -> PublicKey {
    let point = SigningKey::from_slice(&[seed; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(false);
    PublicKey::parse(&crate::auth::encode(point.as_bytes())).unwrap()
}

fn keys(seed: u8) -> RelayKeys {
    let dial = SigningKey::from_slice(&[seed; 32])
        .unwrap()
        .verifying_key()
        .to_sec1_point(true);
    RelayKeys {
        dial: crate::auth::encode(dial.as_bytes()),
        tunnel: crate::auth::encode(&[seed; 32]),
    }
}

/// `keys`, with `seed`'s dial key and `other`'s tunnel key.
fn mixed(seed: u8, other: u8) -> RelayKeys {
    RelayKeys {
        tunnel: keys(other).tunnel,
        ..keys(seed)
    }
}

fn device(id: &str, relay: Option<RelayKeys>) -> NewDevice {
    NewDevice {
        app: None,
        fleet: None,
        id: DeviceId(id.into()),
        name: "iPhone".into(),
        public_key: key(1),
        relay,
        ssh: None,
    }
}

fn add(devices: &mut Devices, id: &str, relay: Option<RelayKeys>) -> Result<()> {
    let via = id.starts_with(FLEET_ID).then(|| format!("{FLEET_VIA}abcd"));
    devices.add_with(device(id, relay), 1, via, Vec::new(), false)
}

fn relay(devices: &Devices, id: &str) -> Option<RelayKeys> {
    devices.get(&DeviceId(id.into())).unwrap().relay.clone()
}

#[test]
fn two_paired_devices_never_share_relay_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    add(&mut devices, "a", Some(keys(10))).unwrap();
    add(&mut devices, "b", None).unwrap();
    for taken in [keys(10), mixed(11, 10), mixed(10, 11)] {
        assert!(matches!(
            devices.set_relay(&DeviceId("b".into()), taken, 2),
            Err(Error::RelayKeysTaken)
        ));
    }
    assert!(matches!(
        add(&mut devices, "c", Some(keys(10))),
        Err(Error::RelayKeysTaken)
    ));
    assert!(devices.get(&DeviceId("c".into())).is_none());
    assert_eq!(relay(&devices, "a"), Some(keys(10)));
    assert_eq!(relay(&devices, "b"), None);
    // Its own keys again, or fresh ones, are fine.
    devices
        .set_relay(&DeviceId("a".into()), keys(10), 3)
        .unwrap();
    devices
        .set_relay(&DeviceId("b".into()), keys(12), 3)
        .unwrap();
}

#[test]
fn a_fleet_device_never_takes_a_paired_devices_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    add(&mut devices, "a", Some(keys(10))).unwrap();
    assert!(matches!(
        add(&mut devices, "fleet-1", Some(keys(10))),
        Err(Error::RelayKeysTaken)
    ));
    add(&mut devices, "fleet-1", None).unwrap();
    assert!(matches!(
        devices.replace_relay(&DeviceId("fleet-1".into()), Some(mixed(11, 10)), 2),
        Err(Error::RelayKeysTaken)
    ));
    assert_eq!(relay(&devices, "a"), Some(keys(10)));
}

#[test]
fn a_paired_device_never_takes_a_fleet_devices_keys() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    add(&mut devices, "fleet-1", Some(keys(10))).unwrap();
    // Posting the same public keys proves nothing about holding their
    // private halves: taking them would cut the fleet phone off.
    add(&mut devices, "a", None).unwrap();
    for taken in [keys(10), mixed(11, 10), mixed(10, 11)] {
        assert!(matches!(
            devices.set_relay(&DeviceId("a".into()), taken, 2),
            Err(Error::RelayKeysTaken)
        ));
    }
    assert!(matches!(
        add(&mut devices, "b", Some(keys(10))),
        Err(Error::RelayKeysTaken)
    ));
    assert_eq!(relay(&devices, "fleet-1"), Some(keys(10)));
    assert_eq!(relay(&devices, "a"), None);
    let reread = Devices::load(dir.path()).unwrap();
    assert_eq!(relay(&reread, "fleet-1"), Some(keys(10)));
}
