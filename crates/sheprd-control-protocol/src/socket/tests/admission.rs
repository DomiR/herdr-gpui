use super::*;
use crate::protocol::{
    Call, Caller, FeedbackRequest, Request, Response, cloud::CloudOp, doc::DocOp, events::EventsOp,
    tabs::TabOp,
};

fn doc_wait() -> Request {
    Request::Doc(Call {
        caller: Caller::default(),
        op: DocOp::Diff {
            path: "/tmp/x.md".into(),
            since: 0,
            wait: 600,
            section: None,
            todos: false,
        },
    })
}

fn hold_waits(
    server: &Server,
    path: &Path,
) -> (
    Vec<thread::JoinHandle<crate::Result<Response>>>,
    Vec<Incoming>,
) {
    let mut callers = Vec::new();
    let mut held = Vec::new();
    for _ in 0..WAIT_CONNECTIONS {
        let request = doc_wait();
        let path = path.to_owned();
        callers.push(thread::spawn(move || call(&path, &request)));
        held.push(server.next().expect("the wait reaches the UI"));
    }
    (callers, held)
}

#[test]
fn long_waits_leave_room_for_a_short_request() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let (callers, held) = hold_waits(&server, &address.path);

    let path = address.path.clone();
    let status = thread::spawn(move || call(&path, &Request::Status(Caller::default())));
    let incoming = server
        .next()
        .expect("a short request reaches the UI while long waits are held");
    incoming.respond(Response::Reloaded { tabs: 0 });
    assert_eq!(
        status.join().unwrap().unwrap(),
        Response::Reloaded { tabs: 0 }
    );

    for incoming in held {
        incoming.respond(Response::Reloaded { tabs: 0 });
    }
    for caller in callers {
        caller.join().unwrap().unwrap();
    }
}

#[test]
fn a_wait_over_its_bound_is_refused_without_reaching_the_ui() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let (callers, held) = hold_waits(&server, &address.path);

    let path = address.path.clone();
    let refused = thread::spawn(move || call(&path, &doc_wait()));
    assert!(matches!(
        refused.join().unwrap().unwrap(),
        Response::Error {
            code: ErrorCode::Busy,
            ..
        }
    ));
    assert!(server.drain().next().is_none());

    let path = address.path.clone();
    let status = thread::spawn(move || call(&path, &Request::Status(Caller::default())));
    let incoming = server
        .next()
        .expect("a short request reaches the UI after a wait is refused");
    incoming.respond(Response::Reloaded { tabs: 0 });
    assert_eq!(
        status.join().unwrap().unwrap(),
        Response::Reloaded { tabs: 0 }
    );

    for incoming in held {
        incoming.respond(Response::Reloaded { tabs: 0 });
    }
    for caller in callers {
        caller.join().unwrap().unwrap();
    }
}

#[test]
fn a_promoted_wait_holds_only_a_wait_permit_and_returns_it() {
    let (_address, server, client) = pair();
    let (sender, events) = mpsc::sync_channel(QUEUE);
    let short = Pool::new(SHORT_CONNECTIONS);
    let waits = Pool::new(WAIT_CONNECTIONS);
    let permit = short.try_acquire().unwrap();
    let worker_waits = waits.clone();
    let (finished, done) = mpsc::channel();
    let worker = thread::spawn(move || {
        serve(&server, &sender, permit, &worker_waits);
        finished.send(()).unwrap();
    });
    write_line(&client, &doc_wait()).unwrap();
    let Event::Request(incoming) = events.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected request");
    };
    assert_eq!(short.in_use(), 0);
    assert_eq!(waits.in_use(), 1);
    incoming.respond(Response::Reloaded { tabs: 0 });
    let answer: Response = serde_json::from_slice(&read_line(&client).unwrap()).unwrap();
    assert_eq!(answer, Response::Reloaded { tabs: 0 });
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_eq!(short.in_use(), 0);
    assert_eq!(waits.in_use(), 0);
}

#[test]
fn a_parked_short_request_keeps_its_short_permit() {
    let (_address, server, client) = pair();
    let (sender, events) = mpsc::sync_channel(QUEUE);
    let short = Pool::new(SHORT_CONNECTIONS);
    let waits = Pool::new(WAIT_CONNECTIONS);
    let permit = short.try_acquire().unwrap();
    let worker_waits = waits.clone();
    let (finished, done) = mpsc::channel();
    let worker = thread::spawn(move || {
        serve(&server, &sender, permit, &worker_waits);
        finished.send(()).unwrap();
    });
    write_line(&client, &Request::Status(Caller::default())).unwrap();
    let Event::Request(incoming) = events.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("expected request");
    };
    assert_eq!(short.in_use(), 1);
    assert_eq!(waits.in_use(), 0);
    incoming.respond(Response::Reloaded { tabs: 0 });
    let answer: Response = serde_json::from_slice(&read_line(&client).unwrap()).unwrap();
    assert_eq!(answer, Response::Reloaded { tabs: 0 });
    done.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert_eq!(short.in_use(), 0);
    assert_eq!(waits.in_use(), 0);
}

#[test]
fn pools_release_on_drop() {
    let pool = Pool::new(2);
    let first = pool.try_acquire().unwrap();
    let second = pool.try_acquire().unwrap();
    assert_eq!(pool.in_use(), 2);
    assert!(pool.try_acquire().is_none());
    drop(first);
    assert!(pool.try_acquire().is_some());
    drop(second);
}

#[test]
fn long_wait_classifies_each_family() {
    let doc = |op| {
        Request::Doc(Call {
            caller: Caller::default(),
            op,
        })
    };
    assert!(
        doc(DocOp::Diff {
            path: "/tmp/x.md".into(),
            since: 0,
            wait: 1,
            section: None,
            todos: false,
        })
        .long_wait()
    );
    assert!(
        !doc(DocOp::Diff {
            path: "/tmp/x.md".into(),
            since: 0,
            wait: 0,
            section: None,
            todos: false,
        })
        .long_wait()
    );
    assert!(
        !doc(DocOp::Read {
            path: "/tmp/x.md".into(),
            section: None,
        })
        .long_wait()
    );
    assert!(
        !doc(DocOp::Open {
            path: "/tmp/x.md".into(),
            line: None,
        })
        .long_wait()
    );

    assert!(
        !Request::Feedback(FeedbackRequest {
            pane_id: "p".into(),
            daemon_socket: None,
            wait_seconds: 0,
        })
        .long_wait()
    );
    assert!(
        Request::Feedback(FeedbackRequest {
            pane_id: "p".into(),
            daemon_socket: None,
            wait_seconds: 30,
        })
        .long_wait()
    );

    let event = |wait_seconds| {
        Request::Events(Call {
            caller: Caller::default(),
            op: EventsOp::Poll {
                after: None,
                instance: None,
                wait_seconds,
                kinds: Vec::new(),
            },
        })
    };
    assert!(!event(0).long_wait());
    assert!(event(5).long_wait());
    assert!(
        Request::Cloud(Call {
            caller: Caller::default(),
            op: CloudOp::Wait {
                batch: 1,
                wait_seconds: 1,
            },
        })
        .long_wait()
    );
    assert!(!Request::Status(Caller::default()).long_wait());
    assert!(
        !Request::Tab(Call {
            caller: Caller::default(),
            op: TabOp::List { pages: false },
        })
        .long_wait()
    );
}
