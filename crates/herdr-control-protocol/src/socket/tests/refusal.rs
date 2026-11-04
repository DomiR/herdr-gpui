use super::*;
use std::io::{self, Write};

struct FailingWriter(fn() -> io::Error);

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err((self.0)())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn request() -> Request {
    Request::Status(Caller::default())
}

fn line(response: &Response) -> Vec<u8> {
    let mut line = serde_json::to_vec(response).unwrap();
    line.push(b'\n');
    line
}

#[test]
fn broken_pipe_recovers_a_busy_answer() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    assert!(matches!(
        exchange(
            FailingWriter(|| io::ErrorKind::BrokenPipe.into()),
            line(&busy).as_slice(),
            &request()
        ),
        Ok(response) if response == busy
    ));
}

#[test]
fn broken_pipe_with_eof_keeps_the_write_error() {
    let error = exchange(
        FailingWriter(|| io::ErrorKind::BrokenPipe.into()),
        &[][..],
        &request(),
    )
    .expect_err("the empty reader cannot recover a response");
    assert!(matches!(error, crate::Error::Io(error) if error.kind() == io::ErrorKind::BrokenPipe));
}

#[test]
fn broken_pipe_with_another_response_keeps_the_write_error() {
    let response = Response::Reloaded { tabs: 0 };
    let error = exchange(
        FailingWriter(|| io::ErrorKind::BrokenPipe.into()),
        line(&response).as_slice(),
        &request(),
    )
    .expect_err("only Busy answers are recovered");
    assert!(matches!(error, crate::Error::Io(error) if error.kind() == io::ErrorKind::BrokenPipe));
}

#[test]
fn a_timed_out_write_does_not_wait_for_an_answer() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    let make_errors: [fn() -> io::Error; 2] = [
        || io::ErrorKind::TimedOut.into(),
        || io::ErrorKind::WouldBlock.into(),
    ];
    for make_error in make_errors {
        let error = exchange(
            FailingWriter(make_error),
            line(&busy).as_slice(),
            &request(),
        )
        .expect_err("a write timeout must not read an answer");
        assert!(matches!(error, crate::Error::Io(error) if error.kind() == make_error().kind()));
    }
}

#[test]
fn not_connected_write_recovers_a_busy_answer() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    assert!(matches!(
        exchange(
            FailingWriter(|| io::ErrorKind::NotConnected.into()),
            line(&busy).as_slice(),
            &request()
        ),
        Ok(response) if response == busy
    ));
}

#[test]
fn uncategorized_write_error_recovers_a_busy_answer() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    assert!(matches!(
        exchange(
            FailingWriter(|| io::Error::other("teardown")),
            line(&busy).as_slice(),
            &request()
        ),
        Ok(response) if response == busy
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn eprototype_write_error_recovers_a_busy_answer() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    assert!(matches!(
        exchange(
            // EPROTOTYPE on Darwin.
            FailingWriter(|| io::Error::from_raw_os_error(41)),
            line(&busy).as_slice(),
            &request()
        ),
        Ok(response) if response == busy
    ));
}

#[test]
fn normal_exchange_is_unchanged() {
    let response = Response::Reloaded { tabs: 0 };
    assert!(matches!(
        exchange(Vec::new(), line(&response).as_slice(), &request()),
        Ok(answer) if answer == response
    ));
}
