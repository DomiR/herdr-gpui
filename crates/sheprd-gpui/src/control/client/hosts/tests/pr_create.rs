#[test]
fn create_help_says_the_user_confirms_without_exporting_credentials() {
    let help = super::super::super::help::usage(Some("pr"));
    assert!(help.contains("sheprd pr create"));
    assert!(help.contains("Nothing is pushed or created by this command"));
    assert!(help.contains("click"));
}
