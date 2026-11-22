#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_brief_is_written_privately_and_discarded() {
    let brief = Brief::write("# Task\nFix the relay", Some(Agent::Claude)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&brief.file).unwrap(),
        "# Task\nFix the relay"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&brief.file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    brief.discard();
    assert!(!brief.file.exists());
}

#[test]
fn the_program_is_told_the_file_and_who_wrote_it() {
    let brief = Brief {
        file: PathBuf::from("/tmp/b.md"),
        from: Some(Agent::Codex),
    };
    assert_eq!(
        brief.args(),
        ["--brief-file", "/tmp/b.md", "--brief-from", "codex"]
    );
    let mut argv = vec![
        "/opt/sheprd-acp".to_owned(),
        "--choose".to_owned(),
        "codex".to_owned(),
    ];
    brief.insert_into(&mut argv);
    assert_eq!(
        argv,
        [
            "/opt/sheprd-acp",
            "--choose",
            "--brief-file",
            "/tmp/b.md",
            "--brief-from",
            "codex",
            "codex"
        ]
    );
    let unknown = Brief {
        from: None,
        ..brief
    };
    assert_eq!(unknown.args(), ["--brief-file", "/tmp/b.md"]);
}

#[test]
fn a_typed_line_reads_the_file_from_the_environment() {
    let brief = Brief {
        file: PathBuf::from("/a path/with 'quotes'.md"),
        from: Some(Agent::Claude),
    };
    let words = brief.line_words();
    assert!(!words.contains("quotes"), "{words}");
    if cfg!(windows) {
        assert_eq!(
            words,
            "--brief-file $env:SHEPRD_BRIEF_FILE --brief-from claude "
        );
    } else {
        assert_eq!(
            words,
            "--brief-file \"$SHEPRD_BRIEF_FILE\" --brief-from claude "
        );
    }
}
