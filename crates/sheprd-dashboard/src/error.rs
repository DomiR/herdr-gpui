#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("could not draw random bytes")]
    Random(#[source] getrandom::Error),
    #[error("{name} must be at least {min} characters")]
    WeakSecret { min: usize, name: &'static str },
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
