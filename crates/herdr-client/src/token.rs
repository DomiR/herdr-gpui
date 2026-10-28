//! What this process's Windows access token says about it: its user, its
//! mandatory integrity level, and whether it is elevated. Neither the
//! standard library nor `interprocess` reads a token, so each query goes
//! through `windows-sys`, with every `unsafe` block kept to the one call that
//! needs it.
#![allow(unsafe_code)]

use std::{
    io,
    os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle},
    ptr,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_ELEVATION,
        TOKEN_INFORMATION_CLASS, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenElevation,
        TokenIntegrityLevel,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

/// One class of information about this process's token, in `u64` units so
/// the buffer is aligned for the structure and the SIDs it points into.
pub(crate) fn information(class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<u64>> {
    let mut raw: HANDLE = ptr::null_mut();
    // SAFETY: the current-process pseudo handle needs no closing, and `raw`
    // is a live out pointer.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: on success `raw` is a token handle this function now owns.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut needed = 0;
    // SAFETY: a null buffer of length zero only asks for the size needed.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8).max(1)];
    let len = u32::try_from(buffer.len() * 8).map_err(|_| io::ErrorKind::InvalidData)?;
    // SAFETY: `buffer` is writable for `len` bytes and outlives the call.
    let filled = unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            class,
            buffer.as_mut_ptr().cast(),
            len,
            &mut needed,
        )
    };
    if filled == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

/// The relative ID of this process's integrity label, such as `0x2000` for
/// medium: the last subauthority of the label's SID.
pub(crate) fn integrity_rid() -> io::Result<u32> {
    let buffer = information(TokenIntegrityLevel)?;
    // SAFETY: the buffer was filled with a `TOKEN_MANDATORY_LABEL` whose SID
    // points into the same buffer, which lives until this function returns.
    // A label SID always has at least one subauthority; a count of zero is
    // refused rather than indexed.
    unsafe {
        let label = &*buffer.as_ptr().cast::<TOKEN_MANDATORY_LABEL>();
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid);
        let last = u32::from(count)
            .checked_sub(1)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(*GetSidSubAuthority(sid, last))
    }
}

/// Whether this process runs elevated, as from an administrator terminal.
pub(crate) fn elevated() -> io::Result<bool> {
    let buffer = information(TokenElevation)?;
    // SAFETY: the buffer was filled with a `TOKEN_ELEVATION` and is aligned
    // for it.
    let elevation = unsafe { &*buffer.as_ptr().cast::<TOKEN_ELEVATION>() };
    Ok(elevation.TokenIsElevated != 0)
}
