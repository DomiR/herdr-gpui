use super::*;

#[test]
fn creating_only_asks_and_carries_no_credentials_or_confirmation_bypass() {
    let op = HostOp::PrCreate {};
    assert_eq!(serde_json::to_string(&op).unwrap(), r#"{"op":"pr_create"}"#);
    assert_eq!(
        serde_json::from_str::<HostOp>(r#"{"op":"pr_create"}"#).unwrap(),
        op
    );
    assert_eq!(op.wait_seconds(), 0);
    for extra in ["confirmed", "token", "checkout", "branch"] {
        let value = serde_json::json!({"op": "pr_create", extra: true});
        assert!(serde_json::from_value::<HostOp>(value).is_err());
    }
}
