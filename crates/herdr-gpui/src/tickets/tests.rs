#![allow(clippy::unwrap_used)]
use super::*;

mod api;
mod board;
mod native;
mod server;

#[test]
fn the_board_defaults_to_herdr_dogs_development_server() {
    let url = Config::default().url().unwrap();
    assert_eq!(url.as_str(), DEFAULT_URL);
}

#[test]
fn a_configured_board_address_wins() {
    let config: Config = toml::from_str(r#"url = "http://127.0.0.1:4173/tickets""#).unwrap();
    assert_eq!(
        config.url().unwrap().as_str(),
        "http://127.0.0.1:4173/tickets"
    );
}

#[test]
fn only_web_addresses_and_known_keys_are_accepted() {
    assert!(toml::from_str::<Config>(r#"url = "file:///etc/passwd""#).is_err());
    assert!(toml::from_str::<Config>(r#"port = 7778"#).is_err());
}

#[test]
fn the_api_defaults_to_herdr_dogs_server() {
    let config = Config::default();
    assert_eq!(config.api().unwrap().as_str(), DEFAULT_API);
    let config: Config = toml::from_str(
        r#"
        api = "http://127.0.0.1:5000/"
        "#,
    )
    .unwrap();
    assert_eq!(config.api().unwrap().as_str(), "http://127.0.0.1:5000/");
}
