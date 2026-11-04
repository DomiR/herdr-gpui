use super::*;
use gpui::{px, size};

#[test]
fn document_theme_labels_are_fluent_and_searchable() {
    assert_eq!(
        crate::i18n::fl!("settings-documents-theme"),
        "Document theme"
    );
    assert_eq!(
        crate::i18n::fl!("settings-documents-theme-description"),
        concat!(
            "Markdown document tabs use the PlainTasks yellow notepad colors and task ",
            "markers, or follow the app theme."
        )
    );
    assert_eq!(
        crate::i18n::fl!("settings-documents-theme-app"),
        "Follow app theme"
    );
    assert_eq!(
        crate::i18n::fl!("settings-documents-theme-plaintasks"),
        "PlainTasks yellow notepad"
    );
    assert!(
        catalog::search("yellow notepad")
            .iter()
            .any(|index| catalog::CATALOG[*index].id == "theme-documents")
    );
    assert!(
        catalog::search("plaintasks")
            .iter()
            .any(|index| catalog::CATALOG[*index].id == "theme-documents")
    );
}

#[gpui::test]
fn document_theme_row_uses_the_loaded_config_choice(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, _| {
        view.section = Section::Theme;
        view.config.documents.theme = crate::config::DocumentTheme::App;
    });
    cx.simulate_resize(size(px(960.), px(1200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));

    assert!(cx.debug_bounds("theme-documents").is_some());
    assert!(cx.debug_bounds("theme-documents-control-0").is_some());
    assert!(cx.debug_bounds("theme-documents-control-1").is_some());
}
