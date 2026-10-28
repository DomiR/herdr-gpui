use super::*;
use crate::transport::access;
use interprocess::os::windows::{
    ToWtf16 as _, local_socket::ListenerOptionsExt as _, security_descriptor::SecurityDescriptor,
};
use std::io;

#[test]
fn a_refused_pipe_names_the_cause_this_process_token_points_to() {
    // A pipe only SYSTEM may open refuses this process whatever its integrity
    // or elevation, so the refusal is real while the cause depends on the
    // runner's token.
    let path = std::env::temp_dir()
        .join(format!("herdr-pipe-denied-test-{}", std::process::id()))
        .join("herdr-client.sock");
    let name = path.to_string_lossy().into_owned();
    let descriptor =
        SecurityDescriptor::deserialize(&"D:P(A;;GA;;;SY)".to_wtf_16().unwrap()).unwrap();
    let _listener = ListenerOptions::new()
        .name(name.to_ns_name::<GenericNamespaced>().unwrap())
        .security_descriptor(descriptor)
        .create_sync()
        .unwrap();
    let error = Stream::connect(&path).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let token = access::Token::current().unwrap();
    let cause = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<access::Error>());
    match access::explain(token, io::ErrorKind::PermissionDenied.into()) {
        Ok(access::Error::LowIntegrity { .. }) => assert!(
            matches!(cause, Some(access::Error::LowIntegrity { .. })),
            "{error:?}"
        ),
        Ok(access::Error::ElevatedServer { .. }) => assert!(
            matches!(cause, Some(access::Error::ElevatedServer { .. })),
            "{error:?}"
        ),
        // An elevated runner keeps the native refusal unchanged.
        Err(_) => {
            assert!(cause.is_none(), "{error:?}");
            assert_eq!(error.raw_os_error(), Some(5));
        }
    }
}
