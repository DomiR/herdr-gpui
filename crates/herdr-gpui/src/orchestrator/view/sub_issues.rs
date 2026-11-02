//! A Linear issue's sub-issues on its page, under the description: each
//! with its status, identifier, title, and assignee, and Add sub-issue,
//! which opens the new issue dialog under it. A sub-issue the listing holds
//! opens its own page here; one it does not, such as another person's
//! outside the current cycle, opens in Linear.

use super::{Event, OrchestratorView, linear_parts};
use crate::i18n::fl;
use crate::{
    config::corners,
    orchestrator::{Item, ItemKey, Provider, SubItem},
};
use gpui::{prelude::*, *};

impl OrchestratorView {
    /// The section, for a Linear issue that has sub-issues or could have.
    pub(super) fn render_sub_issues(&self, item: &Item, cx: &mut Context<Self>) -> Option<Div> {
        if item.key.source.provider != Provider::Linear {
            return None;
        }
        let tracker = item.tracker.as_ref()?;
        let look = &self.look;
        let theme = &look.theme;
        let count = tracker.progress.map(|progress| {
            let more = if progress.more { "+" } else { "" };
            format!("{}/{}{more}", progress.done, progress.total)
        });
        let parent = item.clone();
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .child(look.label(fl!("orchestrator-linear-sub-issues")))
            .children(count.map(|count| look.muted(count)))
            .child(div().flex_1())
            .when(tracker.id.is_some(), |el| {
                el.child(
                    look.compact_button(
                        "orchestrator-add-sub-issue",
                        fl!("orchestrator-linear-add-sub-issue"),
                    )
                    .debug_selector(|| "orchestrator-add-sub-issue".into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_linear_sub_issue(&parent, window, cx);
                    })),
                )
            });
        let rows = tracker
            .children
            .iter()
            .enumerate()
            .map(|(index, child)| self.render_sub_issue(item, index, child, cx));
        let more = tracker.progress.is_some_and(|progress| progress.more);
        let url = item.url.clone();
        Some(
            div()
                .debug_selector(|| "orchestrator-sub-issues".into())
                .flex()
                .flex_col()
                .gap_1()
                .child(header)
                .when(tracker.children.is_empty(), |el| {
                    el.child(look.muted(fl!("orchestrator-linear-no-sub-issues")))
                })
                .children(rows)
                .when_some(url.filter(|_| more), |el, url| {
                    el.child(
                        div()
                            .id("orchestrator-sub-issues-more")
                            .cursor_pointer()
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("orchestrator-linear-more-in-linear"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(Event::OpenUrl(url.clone()));
                            })),
                    )
                }),
        )
    }

    fn render_sub_issue(
        &self,
        parent: &Item,
        index: usize,
        child: &SubItem,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let hover = theme.active;
        let key = ItemKey {
            source: parent.key.source.clone(),
            native_id: child.identifier.clone(),
        }
        .canonical();
        let listed = self.item(&key).is_some();
        let url = child.url.clone();
        let hue = child
            .status
            .as_ref()
            .map_or(theme.muted, |status| linear_parts::status_hue(look, status));
        let id = child.identifier.clone();
        div()
            .id(("orchestrator-sub-issue", index))
            .debug_selector(move || format!("orchestrator-sub-issue-{id}"))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(corners::SMALL))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .child(look.dot(hue, 8.))
            .child(
                look.mono(child.identifier.clone())
                    .flex_none()
                    .text_color(rgb(theme.muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(child.title.clone()),
            )
            .children(
                child
                    .status
                    .as_ref()
                    .map(|status| look.muted(status.name.clone()).flex_none()),
            )
            .children(
                child
                    .assignee
                    .clone()
                    .map(|assignee| look.muted(assignee).flex_none()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if listed {
                    this.open_detail(key.clone(), cx);
                } else if let Some(url) = &url {
                    cx.emit(Event::OpenUrl(url.clone()));
                }
            }))
    }
}
