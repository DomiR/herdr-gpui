use super::*;

#[test]
fn classic_is_a_plain_local_launch() {
    let options = LaunchOptions::parse(["--classic"]).unwrap();
    assert_eq!(options.mode, LaunchMode::Normal);
    assert!(matches!(options.target, ConnectTarget::Local));
}

#[test]
fn classic_keeps_the_connection_options() {
    let options = LaunchOptions::parse(["--classic", "--session", "work"]).unwrap();
    assert!(matches!(
        options.target,
        ConnectTarget::Session { ref name, development: false } if name == "work"
    ));
}
