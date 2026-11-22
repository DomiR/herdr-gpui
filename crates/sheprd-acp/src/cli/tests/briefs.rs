use super::*;

#[test]
fn a_brief_file_and_its_writer_are_read() {
    let options = run(&[
        "claude",
        "--brief-file",
        "/tmp/brief.md",
        "--brief-from",
        "codex",
    ]);
    assert_eq!(options.brief_file, Some(PathBuf::from("/tmp/brief.md")));
    assert_eq!(options.brief_from, Some(Agent::Codex));
    let plain = run(&["claude"]);
    assert_eq!((plain.brief_file, plain.brief_from), (None, None));
}

#[test]
fn a_brief_flag_without_its_value_or_an_unknown_writer_is_refused() {
    assert_eq!(
        parse(args(&["claude", "--brief-file"])),
        Err(Error::MissingValue("--brief-file"))
    );
    assert_eq!(
        parse(args(&["claude", "--brief-from", "eliza"])),
        Err(Error::UnknownAgent("eliza".to_owned()))
    );
}
