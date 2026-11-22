//! Why the fleet could not do what was asked.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The relay agent's answer is not one this app can read")]
    Agent(#[from] sheprd_relay_agent::Error),
    #[error(transparent)]
    AgentBuild(#[from] super::agent::build::Error),
    #[error("The host's answer is not one this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Build(#[from] crate::host_build::Error),
    #[error("The remote control daemon refused: {0}")]
    Daemon(String),
    #[error(
        "The host's SSH key is not one this computer trusts for it; open it once over SSH to check it"
    )]
    HostKey,
    #[error("The host is not one the fleet can share")]
    Intro(#[source] sheprd_remote_control::control::fleet::Error),
    #[error(transparent)]
    Log(#[from] sheprd_remote_control::control::fleet::Error),
    #[error("Remote control is off on this computer")]
    NoDaemon,
    #[error("The host has no Sheprd remote control to ask")]
    NoProgram,
    #[error("This computer listens on no relay; turn one on in Remote Control first")]
    NoRelays,
    #[error("The other computer's fleet did not admit this one")]
    NotAdmitted,
    #[error(
        "The relay agent runs on the host, but no relay lets it listen; check the relays' listen tokens"
    )]
    NotListening,
    #[error("The relay agent's service did not start on the host")]
    NotRunning,
    #[error("Could not read the account's name on the host")]
    NoUser,
    #[error("Could not reach the remote control daemon")]
    Request(#[source] sheprd_remote_control::control::Error),
    #[error("Could not run sheprd-remote-control on the host")]
    Run(#[source] sheprd_client::Error),
    #[error("The fleet's log is larger than a daemon keeps")]
    TooLarge,
    #[error("Could not check the host's SSH key against this computer's known hosts")]
    Trust(#[source] sheprd_client::Error),
    #[error(
        "This host's SSH key is not checked; the fleet shares only hosts whose key is checked strictly"
    )]
    Unchecked,
    #[error("The remote control daemon answered something else")]
    Unexpected,
    #[error("Could not remove all fleet access on the host; some may remain")]
    Unscrubbed,
    #[error("The fleet cannot reach this kind of device")]
    Unsupported,
    // keep-sorted end
}
