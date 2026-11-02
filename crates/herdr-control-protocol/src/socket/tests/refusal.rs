use super::*;
use std::io::{self, Write};

struct FailingWriter(io::ErrorKind);

impl Write for FailingWriter {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(self.0, "write failed"))
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
            FailingWriter(io::ErrorKind::BrokenPipe),
            line(&busy).as_slice(),
            &request()
        ),
        Ok(response) if response == busy
    ));
}

#[test]
fn broken_pipe_with_eof_keeps_the_write_error() {
    let error = exchange(
        FailingWriter(io::ErrorKind::BrokenPipe),
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
        FailingWriter(io::ErrorKind::BrokenPipe),
        line(&response).as_slice(),
        &request(),
    )
    .expect_err("only Busy answers are recovered");
    assert!(matches!(error, crate::Error::Io(error) if error.kind() == io::ErrorKind::BrokenPipe));
}

#[test]
fn non_recoverable_write_error_does_not_read() {
    let busy = Response::error(ErrorCode::Busy, "busy");
    let error = exchange(
        FailingWriter(io::ErrorKind::PermissionDenied),
        line(&busy).as_slice(),
        &request(),
    )
    .expect_err("only connection closure errors are recovered");
    assert!(
        matches!(error, crate::Error::Io(error) if error.kind() == io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn normal_exchange_is_unchanged() {
    let response = Response::Reloaded { tabs: 0 };
    assert!(matches!(
        exchange(Vec::new(), line(&response).as_slice(), &request()),
        Ok(answer) if answer == response
    ));
}
