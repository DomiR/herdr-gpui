//! The C interface the app calls: one function that answers a request and
//! one that frees the answer. Both are `unsafe` because C hands raw
//! pointers across; safe Rust cannot take a buffer from Swift any other
//! way. Everything past the pointers is safe code ([`crate::respond`]).
#![allow(unsafe_code)]

use crate::{Error, call};
use sheprd_remote_control::control::fleet::saved;

/// The largest request read: a saved log at its bound in base64, and a
/// sync's operations beside it.
pub const MAX_REQUEST: usize = saved::MAX_SAVED / 3 * 4 + 4 + 2 * 1024 * 1024;

/// Answers the JSON request of `request_len` bytes at `request`, writing
/// where the answer is to `response` and `response_len`. Returns 0 once
/// it wrote them, and -1, writing nothing, for a null pointer. The answer
/// is JSON, `{"ok": …}` or `{"error": {"kind": …}}`, and must be passed to
/// [`sheprd_fleet_free`] once read.
///
/// # Safety
///
/// `request` must point to `request_len` readable bytes that stay valid and
/// unchanged during the call, and `response` and `response_len` to
/// writable storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sheprd_fleet_call(
    request: *const u8,
    request_len: usize,
    response: *mut *mut u8,
    response_len: *mut usize,
) -> i32 {
    if request.is_null() || response.is_null() || response_len.is_null() {
        return -1;
    }
    let answer = if request_len > MAX_REQUEST {
        call::refusal(&Error::Bounds)
    } else {
        // SAFETY: the caller passes `request_len` readable bytes at
        // `request`, unchanged for this call; the slice is not kept past it.
        call::respond(unsafe { std::slice::from_raw_parts(request, request_len) })
    };
    let answer = Box::into_raw(answer.into_boxed_slice());
    // SAFETY: both are non-null (checked above) and point to writable
    // storage the caller owns.
    unsafe {
        *response_len = answer.len();
        *response = answer.cast::<u8>();
    }
    0
}

/// Frees an answer [`sheprd_fleet_call`] wrote; null does nothing.
///
/// # Safety
///
/// `response` and `len` must be exactly what one call wrote, and each
/// answer freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sheprd_fleet_free(response: *mut u8, len: usize) {
    if response.is_null() {
        return;
    }
    // SAFETY: `response` and `len` are the boxed slice `sheprd_fleet_call`
    // leaked, which nothing else frees.
    drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(response, len)) });
}
