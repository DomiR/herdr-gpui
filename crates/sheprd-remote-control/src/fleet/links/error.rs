/// Why the devices phones linked could not be brought in line with the log.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Auth(#[from] crate::auth::Error),
    #[error(transparent)]
    Desktop(#[from] crate::desktop::Error),
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
