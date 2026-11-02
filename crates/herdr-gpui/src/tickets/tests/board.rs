use crate::tickets::{
    api::{Priority, Project, Status, Ticket},
    board::{labels, lanes, project, reference},
};
use std::collections::BTreeSet;

fn ticket(number: i64, status: Status, sort_key: f64) -> Ticket {
    Ticket {
        number,
        project_id: 1,
        parent: None,
        title: format!("Ticket {number}"),
        description: String::new(),
        status,
        priority: Priority::None,
        kind: "task".into(),
        size: String::new(),
        assignee: String::new(),
        branch: String::new(),
        labels: Vec::new(),
        pull: None,
        sort_key,
        updated_at: 0.,
    }
}

fn numbers(cards: &[crate::tickets::board::Card<'_>]) -> Vec<i64> {
    cards.iter().map(|card| card.ticket.number).collect()
}

#[test]
fn lanes_follow_status_order_and_sort_by_key() {
    let tickets = [
        ticket(1, Status::Done, 2.),
        ticket(2, Status::Todo, 1.),
        ticket(3, Status::Done, 1.),
        ticket(4, Status::Other, 0.),
    ];
    let lanes = lanes(&tickets, &BTreeSet::new());
    let statuses: Vec<Status> = lanes.iter().map(|(status, _)| *status).collect();
    assert_eq!(statuses, Status::LANES);
    let lane = |status| {
        let (_, cards) = lanes.iter().find(|(lane, _)| *lane == status).unwrap();
        numbers(cards)
    };
    assert_eq!(lane(Status::Done), [3, 1]);
    assert_eq!(lane(Status::Todo), [2]);
    // A status this client does not know is kept off the board.
    let shown: usize = lanes.iter().map(|(_, cards)| cards.len()).sum();
    assert_eq!(shown, tickets.len() - 1);
}

#[test]
fn a_pressed_label_filters_cards_but_not_sub_ticket_progress() {
    let mut parent = ticket(1, Status::Todo, 0.);
    parent.labels = vec!["frontend".into()];
    let mut done = ticket(2, Status::Done, 0.);
    done.parent = Some(1);
    let mut open = ticket(3, Status::Todo, 1.);
    open.parent = Some(1);
    let tickets = [parent, done, open];
    let pressed = BTreeSet::from(["frontend".to_owned()]);
    let lanes = lanes(&tickets, &pressed);
    let cards: Vec<_> = lanes.iter().flat_map(|(_, cards)| cards.iter()).collect();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].ticket.number, 1);
    assert_eq!(cards[0].progress, Some((1, 2)));
    assert_eq!(labels(&tickets), ["frontend"]);
}

#[test]
fn the_board_shows_the_picked_project_while_it_exists_else_the_first_by_name() {
    let projects = [
        Project {
            id: 9,
            name: "zeta".into(),
            prefix: "Z".into(),
        },
        Project {
            id: 4,
            name: "alpha".into(),
            prefix: "A".into(),
        },
    ];
    assert_eq!(project(&projects, Some(9)), Some(9));
    assert_eq!(project(&projects, Some(77)), Some(4));
    assert_eq!(project(&projects, None), Some(4));
    assert_eq!(project(&[], Some(9)), None);
    let mut shown = ticket(12, Status::Todo, 0.);
    shown.project_id = 4;
    assert_eq!(reference(&projects, &shown), "A-12");
    shown.project_id = 5;
    assert_eq!(reference(&projects, &shown), "#12");
}
