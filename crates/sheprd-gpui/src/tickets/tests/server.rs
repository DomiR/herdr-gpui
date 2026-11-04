//! The client against a mock herdr-dog that answers by path and records
//! what it was sent.
use crate::{
    browser::WebUrl,
    tickets::api::{NewTicket, Server, Status},
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

pub(super) type Requests = Arc<Mutex<Vec<String>>>;

pub(super) const PROJECTS: &str =
    r#"{"ok":true,"data":[{"id":1,"identity":"/r","name":"herdr","prefix":"HD","createdAt":0}]}"#;
pub(super) const TICKETS: &str = r#"{"ok":true,"data":[{"number":18,"projectId":1,"parent":null,
    "title":"Open picks","description":"","status":"todo","priority":"high","kind":"feature",
    "size":"","assignee":"","branch":"","labels":[],"pull":null,"sortKey":1,
    "createdAt":0,"updatedAt":0,"version":1}]}"#;

/// A herdr-dog on a free port answering `routes` (request line prefix to
/// body), 404 otherwise, for up to `connections` requests.
pub(super) fn serve(routes: Vec<(&'static str, String)>, connections: usize) -> (WebUrl, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Requests::default();
    let seen = requests.clone();
    thread::spawn(move || {
        for _ in 0..connections {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let request = read_request(&mut stream);
            let body = routes
                .iter()
                .find(|(line, _)| request.starts_with(line))
                .map(|(_, body)| body.clone());
            seen.lock().unwrap().push(request);
            let (status, body) = match body {
                Some(body) => ("200 OK", body),
                None => (
                    "404 Not Found",
                    r#"{"ok":false,"error":{"code":"not_found","message":"no route"}}"#.into(),
                ),
            };
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    let url = WebUrl::try_from(format!("http://{address}/").as_str()).unwrap();
    (url, requests)
}

/// One request, its body included: the client may send the headers and
/// the body apart.
fn read_request(stream: &mut impl Read) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text[..end]
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return text;
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return String::from_utf8_lossy(&bytes).into_owned(),
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
        }
    }
}

#[test]
fn the_client_reads_the_board_and_moves_a_ticket() {
    let (url, requests) = serve(
        vec![
            ("GET /api/tickets/projects ", PROJECTS.into()),
            ("GET /api/tickets?project=1 ", TICKETS.into()),
            (
                "POST /api/tickets/18/move ",
                TICKETS
                    .replacen("[", "", 1)
                    .replacen("}]}", "}}", 1)
                    .replace("todo", "done"),
            ),
        ],
        3,
    );
    let server = Server::new(url);
    let projects = server.projects().unwrap();
    assert_eq!(projects[0].prefix, "HD");
    let tickets = server.tickets(1).unwrap();
    assert_eq!(tickets[0].status, Status::Todo);
    let moved = server.set_status(18, Status::InProgress).unwrap();
    assert_eq!(moved.status, Status::Done);
    let requests = requests.lock().unwrap();
    let post = &requests[2];
    // herdr-dog refuses a mutation without a JSON content type.
    assert!(
        post.to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    assert!(post.ends_with(r#"{"status":"in-progress"}"#));
    assert!(!post.to_ascii_lowercase().contains("origin:"));
}

#[test]
fn a_refused_request_reports_herdr_dogs_reason() {
    let (url, _) = serve(Vec::new(), 1);
    let error = Server::new(url).detail(5).unwrap_err();
    assert!(matches!(
        error,
        crate::tickets::Error::Refused { status: 404, .. }
    ));
    assert_eq!(
        error.to_string(),
        "herdr-dog refused (404 not_found): no route"
    );
}

#[test]
fn an_unreachable_server_is_a_network_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = WebUrl::try_from(format!("http://{address}/").as_str()).unwrap();
    let error = Server::new(url).projects().unwrap_err();
    assert!(matches!(error, crate::tickets::Error::Http(_)));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn creating_and_commenting_send_herdr_dogs_shapes() {
    let ticket = TICKETS.replacen('[', "", 1).replacen("}]}", "}}", 1);
    let comment = r#"{"ok":true,"data":{"id":9,"ticket":18,"replyTo":2,"thread":2,
        "author":{"kind":"operator"},"body":"Yes","createdAt":0,"read":true,"question":false}}"#;
    let (url, requests) = serve(
        vec![
            ("POST /api/tickets ", ticket),
            ("POST /api/tickets/18/comments ", comment.into()),
        ],
        2,
    );
    let server = Server::new(url);
    let created = server
        .create(&NewTicket {
            project_id: 1,
            title: "Open picks".into(),
            description: String::new(),
            status: Status::Todo,
        })
        .unwrap();
    assert_eq!(created.number, 18);
    let reply = server.comment(18, "Yes", Some(2)).unwrap();
    assert_eq!((reply.reply_to, reply.thread), (Some(2), 2));
    let requests = requests.lock().unwrap();
    // An empty description is left out, as the web UI leaves it.
    assert!(requests[0].ends_with(r#"{"projectId":1,"title":"Open picks","status":"todo"}"#));
    assert!(requests[1].ends_with(r#"{"body":"Yes","replyTo":2}"#));
}
