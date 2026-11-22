use super::*;

#[test]
fn japanese_is_available_and_formats_messages() {
    let locale: Locale = "ja".parse().unwrap();
    assert!(available().contains(&locale));
    assert_eq!(endonym(&locale), "日本語");
    show(&locale);

    assert_eq!(fl!("settings-title"), "設定");
    assert_eq!(
        fl!("settings-theme-count", shown = 3, total = 9),
        "\u{2068}9\u{2069} 件中 \u{2068}3\u{2069} 件のテーマ"
    );
    // Only Settings has a Japanese catalog so far.
    assert_eq!(fl!("security-title"), english("security-title", &[]));
}

#[test]
fn system_japanese_preferences_resolve_to_the_catalog() {
    let japanese: Locale = "ja".parse().unwrap();
    for tag in ["ja", "ja-JP"] {
        let requested = [tag.parse().unwrap()];
        assert_eq!(resolve(&Language::System, &requested), japanese);
        assert_eq!(
            resolve(&Language::Locale(requested[0].clone().into()), &requested),
            Locale::FALLBACK
        );
    }
}
