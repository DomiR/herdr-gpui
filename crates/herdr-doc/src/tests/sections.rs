use super::*;
use section::{sections, slug};

const PLAN: &str = "intro\n# Goals\n- [ ] a\n## Next Steps!\n- [ ] b\n#### deep\n- [ ] c\n## Next steps\n### Done\n";

#[test]
fn headings_up_to_three_levels_start_sections_with_slugs() {
    let all = sections(PLAN);
    let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["goals", "next-steps", "next-steps-1", "done"]);
    assert_eq!(
        &PLAN[all[1].range.clone()],
        "## Next Steps!\n- [ ] b\n#### deep\n- [ ] c\n"
    );
    assert_eq!(all[1].title, "Next Steps!");
    assert_eq!(all[3].range.end, PLAN.len());
    assert_eq!(slug("Ünïcode: déjà vu_2 — ok"), "ünïcode-déjà-vu_2--ok");
    assert!(matches!(
        section::find(PLAN, "nope"),
        Err(Error::NoSection { .. })
    ));
    assert_eq!(section::find(PLAN, "#done").unwrap().title, "Done");
}

#[test]
fn agents_in_different_sections_do_not_block_each_other() {
    let mut doc = Document::new("# One\n- [ ] a\n# Two\n- [ ] b\n").unwrap();
    let read = doc.version();
    let first = doc
        .agent_edit(
            &agent("p1"),
            Scope {
                version: read,
                section: Some("one"),
            },
            |text| todo::add(text, "a2", Some("one")).map(|edit| vec![edit]),
        )
        .unwrap();
    assert_eq!(first.others, None);
    // The second agent read the same version and works in another section.
    let second = doc
        .agent_edit(
            &agent("p2"),
            Scope {
                version: read,
                section: Some("two"),
            },
            |text| todo::add(text, "b2", Some("two")).map(|edit| vec![edit]),
        )
        .unwrap();
    assert_eq!(
        doc.text(),
        "# One\n- [ ] a\n- [ ] a2\n# Two\n- [ ] b\n- [ ] b2\n"
    );
    let (since, diff) = second.others.unwrap();
    assert_eq!(since, read);
    assert!(diff.unwrap().contains("+- [ ] a2"));
    assert_eq!(second.version, Version(3));
}

#[test]
fn a_changed_section_still_conflicts() {
    let mut doc = Document::new("# One\n- [ ] a\n# Two\n").unwrap();
    let read = doc.version();
    let at_a = doc.text().find("a\n").unwrap();
    type_at(&mut doc, at_a, "x");
    let result = doc.agent_edit(
        &agent("p"),
        Scope {
            version: read,
            section: Some("one"),
        },
        |_| Ok(vec![Edit::insert(0, "nope")]),
    );
    assert!(matches!(result, Err(Error::Conflict(_))));
    // A section the version did not have conflicts too.
    let result = doc.agent_edit(
        &agent("p"),
        Scope {
            version: read,
            section: Some("three"),
        },
        |_| Ok(vec![Edit::insert(0, "nope")]),
    );
    assert!(matches!(result, Err(Error::Conflict(_))));
    assert_eq!(doc.text(), "# One\n- [ ] xa\n# Two\n");
}
