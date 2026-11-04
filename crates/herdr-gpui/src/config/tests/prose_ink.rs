use super::*;

#[test]
fn prose_is_readable_and_quieter_than_subtext() {
    let ratio = crate::contrast::ratio;
    let mut failures = Vec::new();
    for name in Theme::BUILTIN_NAMES {
        for contrast in [Contrast::Standard, Contrast::High] {
            let theme = Theme::builtin(name)
                .unwrap_or_else(|| panic!("missing theme {name}"))
                .with_contrast(contrast);
            let prose = theme.prose();
            for (label, chrome) in [
                ("background", theme.background),
                ("surface", theme.surface),
                ("active", theme.active),
            ] {
                let actual = ratio(prose, chrome);
                let required = theme.contrast.mark_ratio();
                if actual < required {
                    let message =
                        format!("{name} {contrast:?}: prose contrast against {label} is {actual}");
                    failures.push(format!("{message}, below {required}"));
                }
            }
            if prose == theme.foreground {
                failures.push(format!("{name} {contrast:?}: prose equals foreground"));
            }
            if contrast == Contrast::Standard {
                let background = theme.background;
                let prose_ratio = ratio(prose, background);
                if prose_ratio > ratio(theme.subtext(), background) + 0.05 {
                    failures.push(format!(
                        "{name} {contrast:?}: prose is louder than subtext ({prose_ratio})"
                    ));
                }
                if prose_ratio < ratio(theme.muted, background) - 0.05 {
                    failures.push(format!(
                        "{name} {contrast:?}: prose is quieter than muted ({prose_ratio})"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "prose ink failures:\n{}",
        failures.join("\n")
    );
}
