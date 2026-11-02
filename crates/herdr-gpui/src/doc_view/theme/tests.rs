use super::*;

#[test]
fn app_preserves_the_complete_theme_without_inks() {
    for name in ["Default", "Catppuccin Latte", "Catppuccin Mocha"] {
        let Some(mut app) = Theme::builtin(name) else {
            panic!("missing builtin {name}");
        };
        app.accent = Some(0xff2200);
        app.system_accent = Some(0x123456);
        let (resolved, inks) = resolve(DocumentTheme::App, &app);
        assert_eq!(resolved, app);
        assert_eq!(inks, None);
    }
}

#[test]
fn notebook_has_the_pinned_paper_and_alpha_inks() {
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
    assert_eq!(inks.prose, rgba(0x858266ff).into());
    assert_eq!(inks.pending, rgba(0x000000ff).into());
    assert_eq!(inks.done, rgba(0x00000090).into());
    assert_eq!(inks.heading, rgba(0xbc644aff).into());
    assert_eq!(inks.heading_background, rgba(0x00000010).into());
    assert_eq!(inks.delimiter, rgba(0x00000044).into());
    assert_eq!(inks.link, rgba(0x003e7290).into());
    assert_eq!(inks.separator, rgba(0x00000044).into());
    assert_eq!(inks.selection, rgba(0x00000029).into());
    assert_eq!(inks.caret, rgba(0x000000ff).into());
    assert_eq!(inks.caret_row, rgba(0xf7dc8688).into());
    assert_eq!(inks.open_marker, rgba(0x00000066).into());
    assert_eq!(inks.done_marker, rgba(0x00723eff).into());
}

#[test]
fn composite_blends_each_byte_with_rounding() {
    assert_eq!(composite(0x123456, 0xfedcba, 0), 0x123456);
    assert_eq!(composite(0x123456, 0xfedcba, 255), 0xfedcba);
    assert_eq!(composite(0xffffff, 0x000000, 128), 0x7f7f7f);
    assert_eq!(composite(0x000000, 0xffffff, 128), 0x808080);
    assert_eq!(composite(0xff0000, 0x0000ff, 128), 0x7f0080);
}

#[test]
fn notebook_does_not_mutate_or_inherit_the_app_accent() {
    let app = Theme {
        accent: Some(0xff0000),
        system_accent: Some(0x00ff00),
        ..Theme::default()
    };
    let before = app.clone();
    let (notebook, _) = resolve(DocumentTheme::PlainTasks, &app);
    assert_eq!(app, before);
    assert_ne!(notebook.primary(), app.primary());
    assert_eq!(notebook.primary(), 0x688890);
    assert_eq!(notebook.palette, app.palette);
    assert_eq!(resolve(DocumentTheme::App, &app).0, before);
}
