//! The Pull requests list's checks column counts checks by state rather
//! than naming the first few cut to four letters.

use super::super::list::check_tally;
use crate::orchestrator::{CheckState, PrStatus, review::Check};

fn status(checks: &[CheckState]) -> PrStatus {
    PrStatus {
        review: None,
        requested: Vec::new(),
        checks: checks
            .iter()
            .enumerate()
            .map(|(index, state)| Check {
                name: format!("check-{index}"),
                state: *state,
            })
            .collect(),
    }
}

#[test]
fn counts_failures_first_then_pending_then_passed() {
    use CheckState::{Failed, Passed, Pending, Skipped};
    let status = status(&[Passed, Skipped, Failed, Passed, Pending, Passed, Failed]);
    assert_eq!(
        check_tally(&status),
        vec![(Failed, 2), (Pending, 1), (Passed, 3)]
    );
}

#[test]
fn leaves_out_states_no_check_is_in() {
    use CheckState::{Passed, Skipped};
    let many: Vec<_> = std::iter::repeat_n(Passed, 30).collect();
    assert_eq!(check_tally(&status(&many)), vec![(Passed, 30)]);
    assert!(check_tally(&status(&[Skipped, Skipped])).is_empty());
    assert!(check_tally(&status(&[])).is_empty());
}
