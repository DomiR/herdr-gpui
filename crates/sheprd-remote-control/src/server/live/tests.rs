use super::*;
use crate::apns::{ContentState, Phase};
use secrecy::ExposeSecret as _;

mod relay;

fn live(event: Event) -> Live {
    Live {
        event,
        pane: "w1-p3".to_owned(),
        state: ContentState {
            options: Vec::new(),
            permission: None,
            phase: Phase::Finished,
            plan_done: None,
            plan_total: None,
            previous: Vec::new(),
            question: None,
            started_at: 0,
            step: String::new(),
            summary: None,
            waiting: 0,
            working: 0,
        },
    }
}

fn registration(json: &str) -> Registration {
    Registration::parse(json.as_bytes()).unwrap()
}

mod detail {
    use super::*;
    use crate::app::config::ActivityDetail;

    const STEP: &str = "Editing secret.rs";

    fn question() -> Live {
        let mut live = live(Event::Update { alert: None });
        live.state.phase = Phase::Question;
        live.state.step = STEP.to_owned();
        live
    }

    #[test]
    fn a_phone_can_ask_for_less_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::web::tests::state(dir.path());
        let generic = registration(r#"{"activity_detail":"generic"}"#);
        assert_ne!(shown(&state, question(), &generic).state.step, STEP);
        let detailed = Registration::default();
        assert_eq!(shown(&state, question(), &detailed).state.step, STEP);
    }

    #[test]
    fn a_phone_cannot_ask_for_more_than_its_computer_allows() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = crate::web::tests::state(dir.path());
        Arc::get_mut(&mut state).unwrap().live_activities = ActivityDetail::Generic;
        let detailed = Registration::default();
        assert_ne!(shown(&state, question(), &detailed).state.step, STEP);
    }
}
