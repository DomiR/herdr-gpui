//! Who a pipe belongs to. Pipe names live in a namespace every local user
//! shares, so a server restricts its pipe to the user it runs as and a client
//! checks the pipe it reached is owned by that same user before sending
//! anything.
//!
//! Neither the standard library nor `interprocess` reads a token's user or a
//! kernel object's owner, so both go through `windows-sys`. Each `unsafe`
//! block is kept to the one call that needs it.
#![allow(unsafe_code)]

use std::{
    io,
    os::windows::io::{AsRawHandle as _, BorrowedHandle},
    ptr,
    sync::OnceLock,
};
use windows_sys::Win32::{
    Foundation::{HLOCAL, LocalFree},
    Security::{
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT},
        OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_USER, TokenUser,
    },
};

/// Memory the system allocated for an out parameter, freed with `LocalFree`.
struct Local(HLOCAL);

impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from a call documented to allocate it
            // with `LocalAlloc`, and this guard is its only owner.
            unsafe { LocalFree(self.0) };
        }
    }
}

/// `sid` in its string form, such as `S-1-5-21-…`, which SDDL takes.
///
/// # Safety
///
/// `sid` must point to a valid SID that outlives the call.
unsafe fn sid_string(sid: PSID) -> io::Result<String> {
    let mut text: *mut u16 = ptr::null_mut();
    // SAFETY: the caller guarantees `sid`; `text` is a live out pointer.
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = Local(text.cast());
    let wide = text.0.cast::<u16>().cast_const();
    // SAFETY: on success `text` is a NUL-terminated wide string the guard
    // above keeps alive until this function returns.
    let len = (0..).take_while(|&i| unsafe { *wide.add(i) } != 0).count();
    // SAFETY: the `len` units before the terminator were just read.
    let units = unsafe { std::slice::from_raw_parts(wide, len) };
    // A SID string is ASCII; anything else is not one.
    String::from_utf16(units).map_err(|_| io::ErrorKind::InvalidData.into())
}

/// The SID of the user this process runs as. A process token's user never
/// changes, so it is read once and kept.
pub fn user_sid() -> io::Result<String> {
    static SID: OnceLock<String> = OnceLock::new();
    if let Some(sid) = SID.get() {
        return Ok(sid.clone());
    }
    let sid = read_user_sid()?;
    Ok(SID.get_or_init(|| sid).clone())
}

fn read_user_sid() -> io::Result<String> {
    let buffer = crate::token::information(TokenUser)?;
    // SAFETY: the buffer was filled with a `TOKEN_USER` whose SID points into
    // the same buffer, which lives until this function returns.
    unsafe {
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        sid_string(user.User.Sid)
    }
}

/// The SID of the user who owns the kernel object behind `handle`. The
/// handle must allow `READ_CONTROL`, which a pipe opened for reading does.
pub fn owner_sid(handle: BorrowedHandle<'_>) -> io::Result<String> {
    let mut owner: PSID = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the handle is borrowed for the call; the out pointers are live,
    // and the parts not asked for are passed as null as documented.
    let status = unsafe {
        GetSecurityInfo(
            handle.as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let descriptor = Local(descriptor);
    if owner.is_null() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    // SAFETY: `owner` points into the descriptor the guard keeps alive.
    let sid = unsafe { sid_string(owner) };
    drop(descriptor);
    sid
}
