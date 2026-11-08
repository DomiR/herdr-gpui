use super::*;

#[test]
fn document_stamps_default_to_true() -> anyhow::Result<()> {
    for text in ["", "[documents]"] {
        assert!(Config::parse(text)?.documents.stamps);
    }
    assert!(Config::parse(DEFAULT_CONFIG)?.documents.stamps);
    assert!(Config::default().documents.stamps);
    Ok(())
}

#[test]
fn document_stamps_parse_a_boolean() -> anyhow::Result<()> {
    assert!(
        !Config::parse("[documents]\nstamps = false")?
            .documents
            .stamps
    );
    assert!(
        Config::parse("[documents]\nstamps = true")?
            .documents
            .stamps
    );
    Ok(())
}

#[test]
fn document_stamps_reject_non_boolean_values() {
    for text in ["[documents]\nstamps = 'false'", "[documents]\nstamps = 1"] {
        assert!(
            matches!(Config::parse(text), Err(crate::Error::Toml(_))),
            "{text}"
        );
    }
}
