//! The C functions, called as the app calls them.
#![allow(unsafe_code)]

use super::*;
use crate::abi::{MAX_REQUEST, sheprd_fleet_call, sheprd_fleet_free};

fn through_c(request: &[u8], len: usize) -> Value {
    let mut response = std::ptr::null_mut();
    let mut response_len = 0;
    // SAFETY: `request` holds `len` bytes, or `len` is past the bound and
    // the bytes are never read; the out pointers are locals.
    let code = unsafe {
        sheprd_fleet_call(
            request.as_ptr(),
            len,
            &raw mut response,
            &raw mut response_len,
        )
    };
    assert_eq!(code, 0);
    // SAFETY: what the call just wrote, read before it is freed once.
    let answer = unsafe { std::slice::from_raw_parts(response, response_len) }.to_vec();
    unsafe { sheprd_fleet_free(response, response_len) };
    serde_json::from_slice(&answer).unwrap()
}

#[test]
fn a_call_through_c_answers_as_respond_does() {
    let fleet = Fleet::new();
    let request = serde_json::to_vec(&json!({
        "call": "apply", "fleet": fleet.fleet(), "me": fleet.p.hex(),
        "ops": fleet.encoded(), "now_ms": NOW,
    }))
    .unwrap();
    let answer = through_c(&request, request.len());
    assert_eq!(answer["ok"]["status"]["join"], "member");
}

#[test]
fn a_request_past_the_bound_is_refused_unread() {
    let answer = through_c(b"{}", MAX_REQUEST + 1);
    assert_eq!(answer["error"]["kind"], "bounds");
}

#[test]
fn null_pointers_are_refused_and_free_takes_null() {
    let mut response = std::ptr::null_mut();
    let mut len = 0;
    // SAFETY: null is checked before anything is read or written.
    unsafe {
        assert_eq!(
            sheprd_fleet_call(std::ptr::null(), 0, &raw mut response, &raw mut len),
            -1
        );
        assert_eq!(
            sheprd_fleet_call(b"{}".as_ptr(), 2, std::ptr::null_mut(), &raw mut len),
            -1
        );
        sheprd_fleet_free(std::ptr::null_mut(), 0);
    }
    assert!(response.is_null());
}

#[test]
fn garbage_is_answered_as_an_error() {
    let answer = through_c(b"\xff\x00garbage", 9);
    assert_eq!(answer["error"]["kind"], "request");
}
