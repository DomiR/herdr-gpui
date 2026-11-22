//! The label table and the Prometheus text.

use crate::{
    labels,
    metrics::{labelled, series, single},
};

labels! {
    /// Variants keep their docs.
    Color {
        /// The first.
        Blue => "blue",
        Red => "red",
    }
}

#[test]
fn a_label_table_lists_each_variant_once_in_order() {
    assert_eq!(Color::ALL, [Color::Blue, Color::Red]);
    assert_eq!(Color::Red.label(), "red");
    assert_eq!(Color::Red.index(), 1);
    assert_eq!(
        labelled(Color::ALL, |color| color.index() as u64, Color::label),
        [("blue", 0), ("red", 1)]
    );
}

#[test]
fn families_carry_help_type_and_fixed_labels() {
    let mut out = String::new();
    single(&mut out, "up", "gauge", "Whether it is.", 1);
    series(
        &mut out,
        "paints_total",
        "counter",
        "Paints.",
        "color",
        Color::ALL,
        Color::label,
        |color| color.index(),
    );
    assert_eq!(
        out,
        "# HELP up Whether it is.\n# TYPE up gauge\nup 1\n\
         # HELP paints_total Paints.\n# TYPE paints_total counter\n\
         paints_total{color=\"blue\"} 0\npaints_total{color=\"red\"} 1\n"
    );
}
