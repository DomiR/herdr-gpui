//! Code blocks, diagrams, and display math in a document, drawn by the
//! chat's visual workers: a fenced block is coloured by its own language,
//! a `mermaid` fence and a `$$` block are drawn as pictures while the caret
//! is outside them, and as their source while it is inside. All of it is
//! prepared off the UI thread and bounded as chat visuals are.

use super::editor::DocEditor;
use crate::{
    chat::visual::{DiagramStyle, MAX_DIAGRAM_SOURCE, Source, VisualId},
    review::highlight::Span,
};
use gpui::{AppContext as _, Context, Window};
use herdr_doc::markdown::{Kind, Line};
use std::{collections::HashSet, ops::Range, sync::Arc};

/// The most source lines one block sends to a worker.
const MAX_BLOCK_LINES: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlockKind {
    Code,
    Diagram,
    Math,
}

/// One block: the line that opens it, its body, and the visual for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Block {
    pub(super) open: usize,
    pub(super) body: Range<usize>,
    /// The closing line, if there is one.
    pub(super) close: Option<usize>,
    pub(super) kind: BlockKind,
    pub(super) id: VisualId,
}

impl Block {
    /// Every line the block covers.
    pub(super) fn lines(&self) -> Range<usize> {
        self.open..self.close.map_or(self.body.end, |close| close + 1)
    }

    /// Whether it draws as a picture.
    pub(super) fn is_picture(&self) -> bool {
        self.kind != BlockKind::Code
    }
}

/// The blocks of `text` and the source of each one's visual.
pub(super) fn blocks(text: &str, lines: &[Line]) -> Vec<(Block, Source)> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        let open_text = &text[line.range.clone()];
        let (kind, language, is_close): (BlockKind, String, fn(&Line, &str) -> bool) =
            match line.kind {
                Kind::Fence => {
                    let info = open_text.trim_start().trim_start_matches(['`', '~']).trim();
                    let language = info.split_whitespace().next().unwrap_or("").to_lowercase();
                    let kind = if language == "mermaid" {
                        BlockKind::Diagram
                    } else {
                        BlockKind::Code
                    };
                    (kind, language, |line, _| line.kind == Kind::Fence)
                }
                Kind::Paragraph if open_text.trim() == "$$" => {
                    (BlockKind::Math, String::new(), |_, text| {
                        text.trim() == "$$"
                    })
                }
                _ => {
                    index += 1;
                    continue;
                }
            };
        let close = (index + 1..lines.len())
            .find(|at| is_close(&lines[*at], &text[lines[*at].range.clone()]));
        let body = index + 1..close.unwrap_or(lines.len());
        let source: String = body
            .clone()
            .take(MAX_BLOCK_LINES)
            .map(|at| &text[lines[at].range.clone()])
            .collect::<Vec<_>>()
            .join("\n");
        let id = VisualId::of((kind as u8, &language, &source));
        let visual = match kind {
            BlockKind::Code if language.is_empty() => None,
            BlockKind::Code => Some(Source::Code {
                language,
                source: Arc::from(source),
            }),
            BlockKind::Diagram if source.len() <= MAX_DIAGRAM_SOURCE && close.is_some() => {
                Some(Source::Diagram(Arc::from(source)))
            }
            BlockKind::Math if close.is_some() && !source.trim().is_empty() => Some(Source::Math {
                source: Arc::from(source),
                mode: crate::chat::visual::MathMode::Display,
            }),
            _ => None,
        };
        let next = close.map_or(lines.len(), |close| close + 1);
        if let Some(visual) = visual {
            out.push((
                Block {
                    open: index,
                    body,
                    close,
                    kind,
                    id,
                },
                visual,
            ));
        }
        index = next;
    }
    out
}

impl DocEditor {
    /// Asks for each block's visual and lets go of the others.
    pub(super) fn want_visuals(&mut self, found: Vec<(Block, Source)>) -> Vec<Block> {
        let live: HashSet<VisualId> = found.iter().map(|(block, _)| block.id).collect();
        self.visuals.retain(&live);
        found
            .into_iter()
            .map(|(block, source)| {
                self.visuals.want(block.id, source);
                block
            })
            .collect()
    }

    /// The colours of `line`, a line inside a coloured code block.
    pub(super) fn code_spans(&self, line: usize) -> &[Span] {
        self.prepared
            .blocks
            .iter()
            .find(|block| block.kind == BlockKind::Code && block.body.contains(&line))
            .and_then(|block| {
                self.visuals
                    .code(block.id)?
                    .get(line - block.body.start)
                    .map(Vec::as_slice)
            })
            .unwrap_or(&[])
    }

    /// Starts the visuals waiting on the background executor and drops the
    /// textures no block draws any more.
    pub(super) fn drive_visuals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for texture in self.visuals.take_released() {
            cx.drop_image(texture, Some(window));
        }
        let style = DiagramStyle::of(&self.look.theme, &self.look.ui);
        let renderer = cx.svg_renderer();
        for job in self.visuals.jobs(&style, &renderer) {
            let task = cx.background_spawn(async move { job.run() });
            cx.spawn(async move |this, cx| {
                let (id, ticket, drawn) = task.await;
                let _ = this.update(cx, |this, cx| {
                    if this.visuals.finish(id, ticket, drawn) {
                        // A picture changes its row's height.
                        this.list.remeasure();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}

impl DocEditor {
    /// A closed diagram or formula: its picture, or why there is none yet.
    /// A click opens its source.
    pub(super) fn render_picture(
        &mut self,
        line: usize,
        block: &Block,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use crate::{chat::visual::Shown, i18n::fl};
        use gpui::{prelude::*, *};
        let theme = &self.look.theme;
        let start = self.prepared.lines.get(line).map_or(0, |l| l.range.start);
        let body = match self.visuals.shown(block.id) {
            Shown::Ready(drawn) => img(drawn.texture.clone())
                .w(px(drawn.width))
                .h(px(drawn.height))
                .max_w_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            Shown::Pending => div()
                .text_color(rgb(theme.muted))
                .child(fl!("doc-view-drawing"))
                .into_any_element(),
            Shown::Failed(_) => div()
                .text_color(rgb(theme.muted))
                .child(match block.kind {
                    BlockKind::Math => fl!("doc-view-formula-failed"),
                    _ => fl!("doc-view-diagram-failed"),
                })
                .into_any_element(),
        };
        div()
            .id(("doc-picture", line))
            .w_full()
            .pl(px(super::render::GUTTER))
            .pr(px(super::render::PAD))
            .py(px(6.))
            .cursor_pointer()
            .child(body)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    window.focus(&this.focus, cx);
                    cx.stop_propagation();
                    this.select(super::editor::Selection::caret(start), cx);
                    this.reveal(cx);
                }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
