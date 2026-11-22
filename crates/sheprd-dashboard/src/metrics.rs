//! What both relays' counters share: the [`labels!`](crate::labels) table
//! that declares a label set once, the dashboard's login outcomes, and the
//! Prometheus text exposition (format 0.0.4). A label is always one of a
//! fixed set, so no series can carry an address, a token, or anything else
//! a caller chose.

use std::fmt::{Display, Write as _};

/// The `Content-Type` of the Prometheus text exposition.
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Declares a label set once: the enum, its `ALL` list, each label, and the
/// index counters are kept at. Attributes before the name, such as derives
/// and serde's, and before each variant, such as its docs, are kept.
#[macro_export]
macro_rules! labels {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $label:literal,)+ }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $name { $($(#[$variant_meta])* $variant,)+ }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$variant,)+];

            #[must_use]
            pub const fn label(self) -> &'static str {
                match self { $(Self::$variant => $label,)+ }
            }

            /// Where this label's counter sits in an array of `ALL.len()`.
            #[must_use]
            pub const fn index(self) -> usize {
                self as usize
            }
        }
    };
}

labels! {
    /// How a dashboard login went.
    Login {
        Blocked => "blocked",
        Failed => "failed",
        Limited => "limited",
        Succeeded => "succeeded",
    }
}

/// `# HELP` and `# TYPE` for the family `name`.
pub fn family(out: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
}

/// A family of one series without labels.
pub fn single(out: &mut String, name: &str, kind: &str, help: &str, value: impl Display) {
    family(out, name, kind, help);
    let _ = writeln!(out, "{name} {value}");
}

/// A family with one series for each of `all`, labelled `key`.
#[allow(clippy::too_many_arguments)]
pub fn series<L: Copy, V: Display>(
    out: &mut String,
    name: &str,
    kind: &str,
    help: &str,
    key: &str,
    all: &[L],
    label: fn(L) -> &'static str,
    value: impl Fn(L) -> V,
) {
    family(out, name, kind, help);
    for item in all {
        let _ = writeln!(out, "{name}{{{key}=\"{}\"}} {}", label(*item), value(*item));
    }
}

/// Each of `all` by its label, with its value, for a dashboard's tables.
pub fn labelled<L: Copy>(
    all: &[L],
    value: impl Fn(L) -> u64,
    label: fn(L) -> &'static str,
) -> Vec<(&'static str, u64)> {
    all.iter()
        .map(|item| (label(*item), value(*item)))
        .collect()
}
