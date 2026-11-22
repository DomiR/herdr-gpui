use super::*;

#[test]
fn add_link_reads_the_link_from_standard_input_only() {
    let parsed = parse(&["remote", "add-link", "--json", "--window", "2"]).unwrap();
    assert_eq!(
        parsed.command,
        ControlCommand::Remote(RemoteCommand::AddLink)
    );
    assert!(parsed.json);
    assert_eq!(parsed.window, Some(2));
    let refused = parse(&["remote", "add-link", "sheprd-pair:secret"]).unwrap_err();
    assert!(matches!(refused, CliError::InvalidValue(_)));
    assert!(!refused.to_string().contains("secret"), "{refused}");
}
