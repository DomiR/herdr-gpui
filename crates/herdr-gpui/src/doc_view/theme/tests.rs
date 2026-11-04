use super::*;

mod notebook_inks;

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
