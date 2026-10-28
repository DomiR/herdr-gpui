//! Why Windows refused the daemon's pipe, told from this process's token.
//!
//! The pipe is opened for reading and writing, as upstream's own client and
//! any plain pipe client open it, so a refusal is about who is asking, not
//! how. Two policies cause one. A process below medium integrity may not
//! write to an object labeled medium, which a pipe without a label is, nor
//! create files in the user's profile; such a process usually inherits a low
//! label from its executable, as files extracted by a sandboxed program are.
//! And an elevated server labels its pipe high, admitting only elevated
//! clients unless it was started with `--allow-unelevated-clients`.

mod error;

pub use error::Error;

use std::io;

/// `SECURITY_MANDATORY_MEDIUM_RID`: what an ordinary desktop process runs at.
const MEDIUM_RID: u32 = 0x2000;

/// What this process's token says about the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    /// The relative ID of the token's integrity label.
    pub(crate) integrity: u32,
    pub(crate) elevated: bool,
}

impl Token {
    #[cfg(windows)]
    pub(super) fn current() -> io::Result<Self> {
        Ok(Self {
            integrity: crate::token::integrity_rid()?,
            elevated: crate::token::elevated()?,
        })
    }
}

/// The cause `token` points to, or the refusal itself when an elevated
/// process at medium integrity or above was refused, which neither policy
/// explains.
pub(crate) fn explain(token: Token, source: io::Error) -> Result<Error, io::Error> {
    if token.integrity < MEDIUM_RID {
        return Ok(Error::LowIntegrity {
            folder: std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Into::into)),
            source,
        });
    }
    if !token.elevated {
        return Ok(Error::ElevatedServer { source });
    }
    Err(source)
}

/// Name the cause of a refused pipe open, keeping the refusal as its source.
/// Any other error, or one the token cannot explain, passes through as is.
#[cfg(windows)]
pub(super) fn denied(error: io::Error) -> io::Error {
    if error.kind() != io::ErrorKind::PermissionDenied {
        return error;
    }
    let token = match Token::current() {
        Ok(token) => token,
        Err(token_error) => {
            tracing::warn!(operation = "token_query", kind = ?token_error.kind(), raw_os_error = ?token_error.raw_os_error(), "Windows local transport failed");
            return error;
        }
    };
    tracing::warn!(
        integrity = format_args!("{:#x}", token.integrity),
        elevated = token.elevated,
        "Windows refused the daemon pipe"
    );
    match explain(token, error) {
        Ok(cause) => io::Error::new(io::ErrorKind::PermissionDenied, cause),
        Err(error) => error,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
