use super::*;
use sheprd_remote_control::app::pairing::LinkText;

#[test]
fn add_link_carries_the_link_and_never_shows_it() {
    let op = RemoteOp::AddLink {
        link: LinkText("sheprd-pair:secret".into()),
    };
    let text = serde_json::to_string(&op).unwrap();
    assert_eq!(text, r#"{"op":"add_link","link":"sheprd-pair:secret"}"#);
    assert_eq!(serde_json::from_str::<RemoteOp>(&text).unwrap(), op);
    assert!(!format!("{op:?}").contains("secret"));
    assert_eq!(op.wait_seconds(), 0);
}
