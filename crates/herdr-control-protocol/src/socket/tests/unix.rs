use super::*;
use std::io::Write;
use std::os::unix::{
    fs::PermissionsExt as _,
    net::{UnixListener, UnixStream},
};

#[test]
fn only_the_user_can_reach_the_socket() {
    let address = address();
    let _server = Server::bind(&address.path).unwrap();
    let mode = std::fs::metadata(&address.path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_stale_socket_is_replaced() {
    let address = address();
    // A socket file nobody listens on, as a crashed app leaves behind.
    drop(UnixListener::bind(&address.path).unwrap());
    // A process a parallel test forks meanwhile holds a copy of the
    // listener until it execs; wait until nothing answers on it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while UnixStream::connect(&address.path).is_ok() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    Server::bind(&address.path).unwrap();
}

#[test]
fn a_caller_whose_server_already_closed_reads_its_refusal() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    let (caller, mut server) = UnixStream::pair().unwrap();
    let mut line = serde_json::to_vec(&busy).unwrap();
    line.push(b'\n');
    server.write_all(&line).unwrap();
    drop(server);

    let connection = transport::Connection::client(caller, IO_TIMEOUT, IO_TIMEOUT).unwrap();
    let result = exchange(
        &connection,
        &connection,
        &Request::Status(Caller::default()),
    );
    assert!(
        matches!(&result, Ok(response) if response == &busy),
        "{result:?}"
    );
}

#[test]
fn a_refused_caller_reads_busy_rather_than_a_broken_pipe() {
    let address = address();
    let server = Server::bind(&address.path).unwrap();
    let mut callers = Vec::new();
    let mut held = Vec::new();

    for _ in 0..SHORT_CONNECTIONS {
        let path = address.path.clone();
        callers.push(thread::spawn(move || {
            call(&path, &Request::Status(Caller::default()))
        }));
        held.push(server.next().expect("the status request reaches the UI"));
    }

    for attempt in 0..20 {
        let result = call(&address.path, &Request::Status(Caller::default()));
        assert!(
            matches!(
                &result,
                Ok(Response::Error {
                    code: ErrorCode::Busy,
                    ..
                })
            ),
            "refusal {attempt}: {result:?}"
        );
    }

    for incoming in held {
        incoming.respond(Response::Reloaded { tabs: 0 });
    }
    for caller in callers {
        caller.join().unwrap().unwrap();
    }
}
