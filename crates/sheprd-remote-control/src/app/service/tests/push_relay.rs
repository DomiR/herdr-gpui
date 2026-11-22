//! The `[remote] push_relay` setting is on by default and leaves the
//! service definition alone then; turned off, it reaches the daemon as
//! `--no-push-relay`.

use super::*;

#[test]
fn only_turning_the_push_relay_off_becomes_a_flag() {
    let mut plan = plan();
    plan.config.push_relay = true;
    let on = words(&plan.args()).join(" ");
    assert!(!on.contains("push-relay"), "{on}");
    plan.config.push_relay = false;
    assert!(words(&plan.args()).contains(&"--no-push-relay"));
}

#[test]
fn the_push_relay_is_on_unless_the_config_file_turns_it_off() {
    assert!(RemoteConfig::default().push_relay);
    let config: RemoteConfig = toml::from_str("").unwrap();
    assert!(
        config.push_relay,
        "a config written before keeps the default"
    );
    let config: RemoteConfig = toml::from_str("push_relay = false").unwrap();
    assert!(!config.push_relay);
}
