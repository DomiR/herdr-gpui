/// Why joining the fleet over Bluetooth was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the fleet changed since the dialog showed it; pair again")]
    Changed,
    #[error("the joining computer's fleet keys are not the keys it paired with")]
    NotClaimant,
    #[error("this computer is not a member of a fleet to join")]
    NotMember,
    #[error("that fleet key was removed from the fleet; make a new one")]
    Removed,
    // keep-sorted end
}
