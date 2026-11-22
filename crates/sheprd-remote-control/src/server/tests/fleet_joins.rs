//! A phone's join ends with its pairing: once its device is revoked, the
//! join is not listed, cannot be admitted, and revoking drops it.

use super::*;
use crate::control::{
    DeviceId, Request, Response,
    fleet::{
        Method,
        sync::{Answer, Ask},
    },
};
use crate::web::tests::{
    fleet::{Phone, ask, fleet_id, founded},
    signed_in,
};

/// A phone paired here that asked to join; its device and bundle.
fn joined(state: &web::Shared) -> (DeviceId, Phone) {
    let (device, _) = signed_in(state, 1);
    let phone = Phone::new(7);
    let (_, signature) = phone.join(&fleet_id(state));
    let signature = crate::control::fleet::sync::bytes(&signature)
        .unwrap()
        .try_into()
        .unwrap();
    let fleet = state.fleet.clone().unwrap();
    let inner = fleet.inner();
    fleet
        .joins()
        .ask(
            inner.log.as_ref().unwrap(),
            phone.bundle.clone(),
            signature,
            (device.clone(), "iPhone".into()),
            web::now_ms(),
        )
        .unwrap();
    (device, phone)
}

fn listed(state: &web::Shared) -> usize {
    let Answer::Joins { joins } = ask(state, Ask::Joins) else {
        panic!("no joins");
    };
    joins.len()
}

#[test]
fn a_revoked_phones_join_is_neither_listed_nor_admitted() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (device, phone) = joined(&state);
    assert_eq!(listed(&state), 1);
    // Revoked some other way than the control socket (fleet links, idle
    // expiry): the join stays held, and still goes nowhere.
    assert!(state.auth().revoke(&device).unwrap());
    assert_eq!(listed(&state), 0);
    let (bundle, signature) = phone.join(&fleet_id(&state));
    let refused = crate::fleet::ask::answer(
        &state,
        state.fleet.as_ref().unwrap(),
        Ask::Admit {
            bundle,
            signature,
            method: Method::Qr,
        },
        web::now_ms(),
    );
    assert!(matches!(
        refused,
        Err(crate::fleet::Error::Joins(
            crate::fleet::joins::Error::Unpaired
        ))
    ));
    let fleet = state.fleet.as_ref().unwrap();
    assert!(
        !fleet
            .inner()
            .log
            .as_ref()
            .unwrap()
            .is_member(&phone.bundle.id())
    );
}

#[test]
fn revoking_a_device_drops_its_join() {
    let dir = tempfile::tempdir().unwrap();
    let state = founded(dir.path());
    let (device, phone) = joined(&state);
    let (stop, _) = watch::channel(false);
    let response = control::answer(&state, &stop, Request::Revoke { device });
    assert_eq!(response, Response::Done);
    // Gone, not only hidden: a device paired again with the same id would
    // not bring it back.
    let fleet = state.fleet.as_ref().unwrap();
    assert!(!fleet.joins().pending(&phone.bundle.id(), web::now_ms()));
}
