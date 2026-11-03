use super::*;

#[test]
fn prose_contrast_sits_between_muted_and_subtext() {
    let ratio = crate::contrast::ratio;
    for name in Theme::BUILTIN_NAMES {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = Theme::builtin(name)
                .unwrap_or_else(|| panic!("missing theme {name}"))
                .with_contrast(contrast);
            let prose = theme.prose();
            for background in [theme.background] {
                assert!(
                    ratio(prose, background) >= ratio(theme.muted, background) - 0.01,
                    "{name} {:?}: prose is below muted",
                    contrast
                );
                assert!(
                    ratio(prose, background) <= ratio(theme.subtext(), background) + 0.01,
                    "{name} {:?}: prose is above subtext",
                    contrast
                );
            }
            assert_ne!(prose, theme.foreground, "{name} {:?}", contrast);
        }
    }
}
