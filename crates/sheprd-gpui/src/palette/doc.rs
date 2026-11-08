use super::{Action, Entry};
use crate::{
    SheprdWindow,
    config::{Config, DocumentTheme},
    controls::Command as WindowCommand,
    doc_view::{Command, DocEditor, Outline},
    i18n::fl,
};
use gpui::{App, Context, Entity, Window};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DocAction {
    Run(Command),
    Theme(DocumentTheme),
    Zoom(ZoomStep),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoomStep {
    In,
    Out,
    Reset,
}

pub(super) fn doc_entries(
    editor: &Entity<DocEditor>,
    theme: DocumentTheme,
    cx: &mut App,
) -> Vec<Entry> {
    let (hide_done, outlines) =
        editor.update(cx, |editor, cx| (editor.hide_done(), editor.outline(cx)));
    let mut entries = vec![
        command(
            fl!("palette-document-fold-all"),
            Command::FoldAll,
            "document fold",
        ),
        command(
            fl!("palette-document-unfold-all"),
            Command::UnfoldAll,
            "document fold",
        ),
    ];
    entries.extend((1..=6).map(|level| {
        command(
            fl!("palette-document-fold-to-level", level = level),
            Command::FoldToLevel(level),
            "document fold",
        )
    }));
    entries.extend([
        command(
            fl!("palette-document-toggle-task"),
            Command::ToggleTask,
            "document task",
        ),
        command(
            fl!("palette-document-cancel-task"),
            Command::CancelTask,
            "document task",
        ),
        command(
            fl!("palette-document-find"),
            Command::Find,
            "document find search",
        ),
        command(
            if hide_done {
                fl!("palette-document-show-done")
            } else {
                fl!("palette-document-hide-done")
            },
            Command::ToggleHideDone,
            "document tasks done",
        ),
        zoom(
            fl!("palette-document-zoom-in"),
            ZoomStep::In,
            "document zoom",
        ),
        zoom(
            fl!("palette-document-zoom-out"),
            ZoomStep::Out,
            "document zoom",
        ),
        zoom(
            fl!("palette-document-zoom-reset"),
            ZoomStep::Reset,
            "document zoom",
        ),
    ]);
    entries.extend([
        theme_entry(
            DocumentTheme::PlainTasks,
            theme,
            fl!("palette-document-theme-plaintasks"),
            "document theme plaintasks",
        ),
        theme_entry(
            DocumentTheme::App,
            theme,
            fl!("palette-document-theme-app"),
            "document theme app",
        ),
    ]);

    let offset = entries.len();
    entries.extend(
        outlines
            .into_iter()
            .map(|outline| outline_entry(outline, offset)),
    );
    entries
}

fn command(label: String, command: Command, keywords: &str) -> Entry {
    Entry::with_keywords(
        label,
        String::new(),
        fl!("palette-badge-document"),
        Action::Doc(DocAction::Run(command)),
        None,
        keywords,
    )
}

fn zoom(label: String, step: ZoomStep, keywords: &str) -> Entry {
    Entry::with_keywords(
        label,
        String::new(),
        fl!("palette-badge-document"),
        Action::Doc(DocAction::Zoom(step)),
        None,
        keywords,
    )
}

fn theme_entry(
    theme: DocumentTheme,
    current: DocumentTheme,
    label: String,
    keywords: &str,
) -> Entry {
    let detail = if theme == current {
        fl!("palette-document-theme-current")
    } else {
        String::new()
    };
    Entry::with_keywords(
        label,
        detail,
        fl!("palette-badge-document"),
        Action::Doc(DocAction::Theme(theme)),
        None,
        keywords,
    )
}

fn outline_entry(outline: Outline, offset: usize) -> Entry {
    let badge = if outline.heading {
        fl!("palette-badge-heading")
    } else {
        fl!("palette-badge-fold")
    };
    Entry::with_keywords(
        outline.label,
        fl!("palette-document-line", line = outline.line + 1),
        badge,
        Action::Doc(DocAction::Run(Command::GoToLine(outline.line))),
        outline.parent.map(|parent| offset + parent),
        "document heading fold",
    )
}

impl SheprdWindow {
    pub(super) fn activate_doc(
        &mut self,
        action: DocAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .menu
            .palette
            .as_ref()
            .and_then(|palette| palette.doc.clone())
            .filter(|editor| self.doc_views.values().any(|candidate| candidate == editor))
        else {
            self.dismiss_menu(window, cx);
            return;
        };
        let focus = editor.read(cx).focus.clone();
        self.dismiss_menu(window, cx);
        match action {
            DocAction::Run(command) => {
                editor.update(cx, |editor, cx| editor.run(command, window, cx));
                if command != Command::Find {
                    window.focus(&focus, cx);
                }
            }
            DocAction::Theme(theme) => {
                self.set_document_theme(theme, cx);
                window.focus(&focus, cx);
            }
            DocAction::Zoom(step) => {
                window.focus(&focus, cx);
                let command = match step {
                    ZoomStep::In => WindowCommand::IncreaseFontSize,
                    ZoomStep::Out => WindowCommand::DecreaseFontSize,
                    ZoomStep::Reset => WindowCommand::ResetFontSize,
                };
                self.zoom_focused_page(command, window, cx);
            }
        }
    }

    fn set_document_theme(&mut self, theme: DocumentTheme, cx: &mut Context<Self>) {
        if self.config.documents.theme == theme {
            return;
        }
        self.config.documents.theme = theme;
        cx.notify();
        self.write_preference(
            move || Config::set_key("documents.theme", theme.as_str().into()),
            cx,
        );
    }
}
