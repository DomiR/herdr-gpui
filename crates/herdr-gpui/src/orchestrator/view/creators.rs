//! The Pull requests list by who opened each: a grouping with one heading
//! per author, and pills picking whose pull requests show. None picked
//! shows everyone's; Mine stays its own toggle.

use super::{
    OrchestratorView,
    rows::{Heading, ItemRow, Line, Tab},
};
use crate::i18n::fl;
use crate::{config::corners, orchestrator::Item};
use gpui::{prelude::*, *};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

/// How the Pull requests list is grouped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Grouping {
    /// By what each waits on.
    #[default]
    Status,
    /// By author.
    Creator,
}

impl Grouping {
    pub(crate) const ALL: [Self; 2] = [Self::Status, Self::Creator];

    pub(crate) fn label(self) -> String {
        match self {
            Self::Status => fl!("orchestrator-linear-status"),
            Self::Creator => fl!("orchestrator-linear-grouping-creator"),
        }
    }

    /// Its name in element ids, the same in every language.
    fn key(self) -> &'static str {
        match self {
            Self::Status => "Status",   // i18n: data
            Self::Creator => "Creator", // i18n: data
        }
    }
}

/// Authors by name regardless of case, an unknown one last.
fn by_name(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left
            .to_lowercase()
            .cmp(&right.to_lowercase())
            .then_with(|| left.cmp(right)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Pull requests under their authors, each group in the order the rows came.
/// A heading names its group by the first row's author.
pub(crate) fn creator_lines<'a>(
    rows: &[ItemRow],
    author: impl Fn(&ItemRow) -> Option<&'a str>,
) -> Vec<Line> {
    let mut authors: Vec<Option<&str>> = rows.iter().map(&author).collect();
    authors.sort_by(|left, right| by_name(*left, *right));
    authors.dedup();
    let mut lines = Vec::with_capacity(rows.len() + authors.len());
    for name in authors {
        let grouped: Vec<ItemRow> = rows
            .iter()
            .copied()
            .filter(|row| author(row) == name)
            .collect();
        let Some(&first) = grouped.first() else {
            continue;
        };
        lines.push(Line::Heading {
            heading: Heading::Creator(first),
            count: grouped.len(),
        });
        lines.extend(grouped.into_iter().map(Line::Row));
    }
    lines
}

/// Whether `item` is by one of `people`, or `people` picks no one.
pub(crate) fn picked(people: &HashSet<String>, item: &Item) -> bool {
    people.is_empty()
        || item
            .author
            .as_ref()
            .is_some_and(|author| people.contains(author))
}

impl OrchestratorView {
    /// Each author of the pull requests the search and filters keep, by
    /// name, with how many they opened, and every one picked though none of
    /// theirs is listed, so the pick can be undone. An item several
    /// checkouts list counts once.
    pub(super) fn people(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut counted = HashSet::new();
        for repo in &self.repos {
            for row in &repo.pull_request_rows {
                let item = &repo.snapshot.items[row.item];
                if !counted.insert(item.key.canonical()) {
                    continue;
                }
                if let Some(author) = item.author.as_deref() {
                    *counts.entry(author).or_default() += 1;
                }
            }
        }
        for author in &self.authors {
            counts.entry(author.as_str()).or_default();
        }
        let mut people: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(author, count)| (author.to_owned(), count))
            .collect();
        people.sort_by(|(left, _), (right, _)| by_name(Some(left.as_str()), Some(right.as_str())));
        people
    }

    /// The people pills under the Pull requests search: Everyone, then one
    /// per author, while there is more than one to choose between.
    pub(super) fn render_author_pills(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.tab != Tab::PullRequests {
            return None;
        }
        let people = self.people();
        if people.len() < 2 && self.authors.is_empty() {
            return None;
        }
        let look = &self.look;
        let theme = &look.theme;
        let count = |count: usize| {
            (count > 0).then(|| div().text_color(rgb(theme.muted)).child(count.to_string()))
        };
        let all = self.authors.is_empty();
        let everyone = look
            .pill("orchestrator-pill-authors-everyone", all)
            .child(look.icon("icons/user.svg", 11., theme.subtext()))
            .child(fl!("orchestrator-linear-authors-everyone"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.authors.clear();
                this.refresh_rows();
                cx.notify();
            }));
        let pills = people.into_iter().map(|(author, items)| {
            let on = self.authors.contains(&author);
            look.pill(format!("orchestrator-pill-author-{author}"), on)
                .child(author.clone())
                .children(count(items))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.authors.remove(&author) {
                        this.authors.insert(author.clone());
                    }
                    this.refresh_rows();
                    cx.notify();
                }))
        });
        Some(
            div()
                .debug_selector(|| "orchestrator-author-pills".into())
                .flex_none()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .px_4()
                .pb_2()
                .child(everyone)
                .children(pills),
        )
    }

    /// The sort menu's Group by choices, for the Pull requests list only.
    pub(super) fn render_grouping(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.tab != Tab::PullRequests {
            return None;
        }
        let look = &self.look;
        let theme = &look.theme;
        let choices = Grouping::ALL.into_iter().map(|grouping| {
            let on = grouping == self.grouping;
            let hover = theme.active;
            let id = format!("orchestrator-group-{}", grouping.key());
            div()
                .id(SharedString::from(id.clone()))
                .debug_selector(move || id)
                .px_2()
                .h(px(28.))
                .flex()
                .items_center()
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .when(on, |el| el.bg(rgb(theme.primary_wash())))
                .child(grouping.label())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.grouping = grouping;
                    this.sort_open = false;
                    this.refresh_rows();
                    cx.notify();
                }))
        });
        Some(
            div()
                .flex()
                .flex_col()
                .mt_1()
                .pt_1()
                .border_t_1()
                .border_color(rgb(theme.active))
                .child(look.label("GROUP BY").px_2().pt_1().pb_1())
                .children(choices),
        )
    }
}
