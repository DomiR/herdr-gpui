use super::*;

#[test]
fn notebook_has_the_exact_packed_inks() {
    let (theme, inks) = resolve(DocumentTheme::PlainTasks, &Theme::default());
    let Some(inks) = inks else {
        panic!("Notebook inks");
    };
    assert_eq!(theme.background, 0xefe9b7);
    assert_eq!(theme.foreground, 0x000000);
    assert_eq!(theme.cursor, 0x000000);
    assert_eq!(theme.muted, 0x858266);
    assert_eq!(theme.surface, 0xe0daac);
    assert_eq!(theme.active, 0xc9c49a);
    assert_eq!(theme.primary(), 0x688890);
    assert_eq!(theme.system_accent, None);
    assert_eq!(inks.plain, rgba(0x00000050).into());
    assert_eq!(inks.notes, rgba(0x858266ff).into());
    assert_eq!(inks.pending, rgba(0x000000ff).into());
    assert_eq!(inks.done, rgba(0x00000090).into());
    assert_eq!(inks.cancelled, rgba(0x68654fff).into());
    assert_eq!(inks.open_marker, rgba(0x00000066).into());
    assert_eq!(inks.done_marker, rgba(0x00723eff).into());
    assert_eq!(inks.cancelled_marker, rgba(0xb60101ff).into());
    assert_eq!(inks.tag, rgba(0xa02e2b99).into());
    assert_eq!(inks.tag_meta, rgba(0x00000050).into());
    assert_eq!(inks.url, rgba(0x003e7290).into());
    assert_eq!(inks.delimiter, rgba(0xded9a6ff).into());
    assert_eq!(inks.separator, rgba(0x00000044).into());
    assert_eq!(inks.selection, rgba(0x00000029).into());
    assert_eq!(inks.caret, rgba(0x000000ff).into());
    assert_eq!(inks.caret_row, rgba(0xf7dc8688).into());
    assert_eq!(
        inks.today,
        (rgba(0x000000ff).into(), rgba(0xeadd4eff).into())
    );
    assert_eq!(
        inks.high,
        (rgba(0x000000ff).into(), rgba(0xff7f00ff).into())
    );
    assert_eq!(
        inks.critical,
        (rgba(0x000000ff).into(), rgba(0xff0000ff).into())
    );
    assert_eq!(inks.low, (rgba(0xffffffff).into(), rgba(0x222222ff).into()));
}
