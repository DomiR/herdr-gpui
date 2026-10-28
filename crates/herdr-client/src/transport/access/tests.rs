use super::*;
use std::{error::Error as _, io};

const LOW: u32 = 0x1000;
const HIGH: u32 = 0x3000;

fn refused() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "refused")
}

fn explained(integrity: u32, elevated: bool) -> Result<Error, io::Error> {
    explain(
        Token {
            integrity,
            elevated,
        },
        refused(),
    )
}

#[test]
fn a_process_below_medium_integrity_is_told_to_relabel_its_folder() {
    // Elevation does not lift a label the executable imposes, so a low process
    // started from an elevated task is still low.
    for elevated in [false, true] {
        let error = explained(LOW, elevated).unwrap();
        let Error::LowIntegrity { folder, .. } = &error else {
            panic!("expected low integrity, got {error:?}");
        };
        let folder = folder.as_deref().unwrap();
        assert_eq!(Some(folder), std::env::current_exe().unwrap().parent());
        let text = error.to_string();
        assert!(text.contains("low integrity"), "{text}");
        assert!(
            text.contains(&format!("icacls \"{}\"", folder.display())),
            "{text}"
        );
        assert!(text.contains("/setintegritylevel (OI)(CI)M /T"), "{text}");
    }
    assert!(matches!(
        explained(0, false),
        Ok(Error::LowIntegrity { .. })
    ));
}

#[test]
fn an_ordinary_process_is_pointed_at_the_elevated_server() {
    for integrity in [MEDIUM_RID, 0x2100] {
        let error = explained(integrity, false).unwrap();
        assert!(matches!(error, Error::ElevatedServer { .. }), "{error:?}");
        let text = error.to_string();
        assert!(text.contains("--allow-unelevated-clients"), "{text}");
    }
}

#[test]
fn an_elevated_process_keeps_the_original_refusal() {
    for integrity in [MEDIUM_RID, HIGH] {
        let error = explained(integrity, true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "refused");
    }
}

#[test]
fn the_refusal_stays_the_source() {
    let error = explained(LOW, false).unwrap();
    let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
    assert_eq!(source.to_string(), "refused");
    let error = explained(MEDIUM_RID, false).unwrap();
    let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
    assert_eq!(source.kind(), io::ErrorKind::PermissionDenied);
}
