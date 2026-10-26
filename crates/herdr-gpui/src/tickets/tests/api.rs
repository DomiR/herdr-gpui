use super::*;
use crate::tickets::api::{Author, Comment, Detail, Priority, Start, Status, Ticket, interpret};

#[test]
fn a_ticket_reads_as_herdr_dog_writes_it() {
    let body = br##"{"ok":true,"data":[{"number":18,"projectId":1,"parent":null,
        "title":"Open picks","description":"# Problem","status":"in-progress",
        "priority":"high","kind":"feature","size":"","assignee":"claude",
        "branch":"","labels":["frontend"],"pull":null,"sortKey":1.5,
        "createdAt":1790723561381,"updatedAt":1790723561382,"version":3}]}"##;
    let tickets: Vec<Ticket> = interpret(200, body).unwrap();
    assert_eq!(tickets.len(), 1);
    let ticket = &tickets[0];
    assert_eq!(ticket.number, 18);
    assert_eq!(ticket.status, Status::InProgress);
    assert_eq!(ticket.priority, Priority::High);
    assert_eq!(ticket.labels, ["frontend"]);
}

#[test]
fn values_newer_than_this_client_still_read() {
    let body = br#"{"ok":true,"data":{"ticket":{"number":2,"projectId":1,"parent":1,
        "title":"t","description":"","status":"parked","priority":"urgent","kind":"spike",
        "size":"huge","assignee":"","branch":"","labels":[],"pull":null,"sortKey":0,
        "createdAt":0,"updatedAt":0,"version":1,"extra":true},
        "subTickets":[],"progress":{"done":0,"total":0},
        "relations":[{"type":"blocked_by","ticket":3,"ref":"HD-3","title":"x","status":"todo"}]}}"#;
    let detail: Detail = interpret(200, body).unwrap();
    assert_eq!(detail.ticket.status, Status::Other);
    assert_eq!(detail.ticket.priority, Priority::Other);
    assert_eq!(detail.relations[0].reference, "HD-3");
}

#[test]
fn comments_name_their_author() {
    let body = br#"{"ok":true,"data":[
        {"id":2,"ticket":19,"replyTo":null,"thread":2,"author":{"kind":"agent","name":"claude"},
         "body":"Plan","createdAt":1791023099577,"read":true,"question":false},
        {"id":3,"ticket":19,"replyTo":2,"thread":2,"author":{"kind":"operator"},
         "body":"Yes","createdAt":1791023197808,"read":true,"question":false}]}"#;
    let comments: Vec<Comment> = interpret(200, body).unwrap();
    assert_eq!(
        comments[0].author,
        Author::Agent {
            name: "claude".into()
        }
    );
    assert_eq!(comments[1].author.name(), "You");
    assert_eq!(comments[1].reply_to, Some(2));
}

#[test]
fn a_refusal_keeps_herdr_dogs_code_and_a_bounded_message() {
    let long = "x".repeat(10_000);
    let body = format!(r#"{{"ok":false,"error":{{"code":"not_found","message":"{long}"}}}}"#);
    let error = interpret::<Vec<Ticket>>(404, body.as_bytes()).unwrap_err();
    let Error::Refused {
        status,
        code,
        message,
    } = error
    else {
        panic!("expected a refusal, got {error:?}");
    };
    assert_eq!((status, code.as_str()), (404, "not_found"));
    assert!(message.chars().count() <= 240);
}

#[test]
fn an_answer_that_is_not_the_envelope_is_an_error() {
    assert!(matches!(
        interpret::<Vec<Ticket>>(200, b"<html>"),
        Err(Error::Json(_))
    ));
    assert!(matches!(
        interpret::<Vec<Ticket>>(200, br#"{"ok":true}"#),
        Err(Error::Json(_))
    ));
}

#[test]
fn a_ticket_never_started_has_no_start() {
    let start: Option<Start> = interpret(200, br#"{"ok":true,"data":null}"#).unwrap();
    assert_eq!(start, None);
}

#[test]
fn only_a_worktree_on_this_machine_opens_here() {
    let read = |key: &str| -> Start {
        let body = format!(
            r#"{{"ok":true,"data":{{"ticket":19,"stage":"prompted","branch":"b",
            "worktreeKey":{key},"error":null,"recovery":null,"startedAt":0,"updatedAt":0}}}}"#
        );
        interpret::<Option<Start>>(200, body.as_bytes())
            .unwrap()
            .unwrap()
    };
    let local = read(r#""/Users/me/.herdr/worktrees/repo/b""#);
    assert_eq!(
        local.local_worktree(),
        Some("/Users/me/.herdr/worktrees/repo/b")
    );
    assert_eq!(read(r#""ssh:box:/home/me/repo/b""#).local_worktree(), None);
    assert_eq!(read("null").local_worktree(), None);
}
