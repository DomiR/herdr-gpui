//! Drawing parsed Markdown in the window's theme: styled runs in one text
//! element per paragraph, links that open in the browser when clicked, and
//! images once `super::images` has read them.

use super::{
    images,
    parse::{Block, Inline, ListItem},
};
use crate::{config::corners, fonts::StyledFont, orchestrator::Look};
use gpui::{prelude::*, *};

/// The tallest an image draws; a wider one scales down to the column.
const MAX_IMAGE_HEIGHT: f32 = 480.;

/// Numbers each interactive element of one drawing, so their ids differ.
struct Ids {
    prefix: SharedString,
    next: usize,
}

impl Ids {
    fn next(&mut self) -> ElementId {
        self.next += 1;
        (self.prefix.clone(), self.next).into()
    }
}

/// `blocks` as one column, named `id` for its elements.
pub(crate) fn render(id: impl Into<SharedString>, blocks: &[Block], look: &Look, cx: &App) -> Div {
    let mut ids = Ids {
        prefix: id.into(),
        next: 0,
    };
    column(blocks, look, &mut ids, cx)
}

/// One block alone, for views that place each block themselves; `index`
/// keeps its ids apart from its neighbours'.
pub(crate) fn block(
    id: impl Into<SharedString>,
    index: usize,
    block: &Block,
    look: &Look,
    cx: &App,
) -> AnyElement {
    let mut ids = Ids {
        prefix: SharedString::from(format!("{}-{index}", id.into())),
        next: 0,
    };
    draw(block, look, &mut ids, cx)
}

fn column(blocks: &[Block], look: &Look, ids: &mut Ids, cx: &App) -> Div {
    div()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_2()
        .children(blocks.iter().map(|block| draw(block, look, ids, cx)))
}

fn draw(block: &Block, look: &Look, ids: &mut Ids, cx: &App) -> AnyElement {
    let theme = &look.theme;
    match block {
        Block::Paragraph(inline) => div()
            .min_w_0()
            .child(text(inline, look, ids))
            .into_any_element(),
        Block::Heading(level, inline) => {
            let grow = match level {
                1 => 6.,
                2 => 4.,
                3 => 2.,
                _ => 1.,
            };
            div()
                .min_w_0()
                .pt_1()
                .text_size(px(look.ui.size + grow))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme.primary()))
                .child(text(inline, look, ids))
                .into_any_element()
        }
        Block::Code(code) => div()
            .id(ids.next())
            .min_w_0()
            .px_3()
            .py_2()
            .rounded(px(corners::SMALL))
            .bg(rgb(theme.active))
            .text_font(&look.mono)
            .text_size(look.small())
            .overflow_x_scroll()
            .children(
                code.lines()
                    .map(|line| div().whitespace_nowrap().child(line.to_owned())),
            )
            .into_any_element(),
        Block::Quote(blocks) => div()
            .min_w_0()
            .pl_3()
            .border_l_2()
            .border_color(rgb(theme.active))
            .text_color(rgb(theme.subtext()))
            .child(column(blocks, look, ids, cx))
            .into_any_element(),
        Block::List { start, items } => div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .children(items.iter().enumerate().map(|(index, item)| {
                let marker = match (item.task, start) {
                    (Some(true), _) => "☑".to_owned(),
                    (Some(false), _) => "☐".to_owned(),
                    (None, Some(start)) => format!("{}.", start + index as u64),
                    (None, None) => "•".to_owned(),
                };
                list_item(marker, item, look, ids, cx)
            }))
            .into_any_element(),
        Block::Rule => div()
            .my_1()
            .h(px(1.))
            .bg(rgb(theme.active))
            .into_any_element(),
        Block::Image { url, alt } => image(url, alt, look, ids, cx),
        Block::Table { head, rows } => {
            let row = |cells: &[Inline], heading: bool, ids: &mut Ids| {
                div().flex().min_w_0().children(cells.iter().map(|cell| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_2()
                        .py_1()
                        .border_1()
                        .border_color(rgb(theme.active))
                        .when(heading, |cell| cell.font_weight(FontWeight::SEMIBOLD))
                        .child(text(cell, look, ids))
                }))
            };
            let mut table = div().min_w_0().flex().flex_col().text_size(look.small());
            table = table.child(row(head, true, ids));
            for cells in rows {
                table = table.child(row(cells, false, ids));
            }
            table.into_any_element()
        }
    }
}

fn list_item(marker: String, item: &ListItem, look: &Look, ids: &mut Ids, cx: &App) -> Div {
    div()
        .min_w_0()
        .flex()
        .gap_2()
        .child(
            div()
                .flex_none()
                .min_w(px(14.))
                .text_color(rgb(look.theme.muted))
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(column(&item.blocks, look, ids, cx)),
        )
}

fn image(url: &str, alt: &str, look: &Look, ids: &mut Ids, cx: &App) -> AnyElement {
    let opens = images::readable(url).then(|| url.to_owned());
    let shown = match images::image(url, cx) {
        Some(image) => img(image)
            .max_w_full()
            .max_h(px(MAX_IMAGE_HEIGHT))
            .object_fit(ObjectFit::ScaleDown)
            .into_any_element(),
        None => {
            let name = if alt.is_empty() { "image" } else { alt };
            div()
                .px_2()
                .py_1()
                .rounded(px(corners::SMALL))
                .border_1()
                .border_color(rgb(look.theme.active))
                .text_size(look.small())
                .text_color(rgb(look.theme.muted))
                .child(format!("[{name}]"))
                .into_any_element()
        }
    };
    div()
        .id(ids.next())
        .min_w_0()
        .flex()
        .when_some(opens, |frame, url| {
            frame
                .cursor_pointer()
                .on_click(move |_, _, cx| cx.open_url(&url))
        })
        .child(shown)
        .into_any_element()
}

/// One paragraph's text with its styled runs; links open when clicked.
fn text(inline: &Inline, look: &Look, ids: &mut Ids) -> AnyElement {
    let theme = &look.theme;
    let accent = theme.ink(theme.palette[4]);
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = inline
        .spans
        .iter()
        .map(|span| {
            let mut style = HighlightStyle::default();
            if span.style.bold {
                style.font_weight = Some(FontWeight::BOLD);
            }
            if span.style.italic {
                style.font_style = Some(FontStyle::Italic);
            }
            if span.style.strike {
                style.strikethrough = Some(StrikethroughStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            if span.style.code {
                style.background_color = Some(rgb(theme.active).into());
                style.color = Some(rgb(theme.foreground).into());
            }
            if span.link.is_some() {
                style.color = Some(rgb(accent).into());
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            (span.range.clone(), style)
        })
        .collect();
    let styled = StyledText::new(inline.text.clone()).with_highlights(highlights);
    let links: Vec<(std::ops::Range<usize>, String)> = inline
        .spans
        .iter()
        .filter_map(|span| Some((span.range.clone(), span.link.clone()?)))
        .collect();
    if links.is_empty() {
        return styled.into_any_element();
    }
    let ranges = links.iter().map(|(range, _)| range.clone()).collect();
    let urls: Vec<String> = links.into_iter().map(|(_, url)| url).collect();
    InteractiveText::new(ids.next(), styled)
        .on_click(ranges, move |clicked, _, cx| {
            if let Some(url) = urls.get(clicked)
                && !crate::orchestrator::open_linear_link(url, cx)
            {
                cx.open_url(url);
            }
        })
        .into_any_element()
}
