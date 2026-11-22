//! The window in pieces, for a host that lays them out itself: each region
//! draws through the same methods as the whole window, so state and listeners
//! stay on `SheprdWindow`. The sizes a layout needs come from [`Sizes`], since
//! only this view knows them.

use super::SheprdWindow;
use crate::fonts::StyledFont as _;
use crate::sidebar::{ActivityPanel, BAR_WIDTH, SidebarMode};
use gpui::{AnyElement, AnyView, Context, Div, Empty, Window, div, prelude::*, px, rgb};

/// A piece of the window a host places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    ActivityBar,
    /// The device picker under the sidebar's lists, as the sidebar ends with
    /// it, while `[sidebar] device_footer` shows it.
    Devices,
    /// The row over the activity bar and sidebar: the traffic lights' room,
    /// and Back and Forward.
    Header,
    /// Menus, toasts and what else draws over the window, in its coordinates,
    /// so the host paints it over the whole window, outside its layout. It takes
    /// no clicks of its own.
    Overlay,
    Sidebar,
    StatusBar,
    /// What ends the title bar: the checkout's changes and branch, the
    /// account, and the details toggle. The tab strips leave its room free.
    TitleBar,
}

/// The sizes the regions want, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub activity_bar: f32,
    /// The device picker's height, or zero while it is hidden.
    pub devices: f32,
    pub header: f32,
    /// The window's height, which the left column's sections share.
    pub height: f32,
    /// The column holding the activity bar and sidebar.
    pub left: f32,
    pub status_bar: f32,
    /// The title bar's end, as last drawn.
    pub titlebar: f32,
}

impl SheprdWindow {
    pub(crate) fn region_sizes(&self, window: &Window) -> Sizes {
        let width = f32::from(window.viewport_size().width);
        Sizes {
            activity_bar: if self.activity_bar_leads() {
                BAR_WIDTH
            } else {
                0.
            },
            devices: if self.device_footer_shown() {
                crate::sidebar::DEVICE_FOOTER_HEIGHT
            } else {
                0.
            },
            header: crate::titlebar::HEIGHT,
            height: f32::from(window.viewport_size().height),
            left: self.sidebar_column_width(width).unwrap_or(0.),
            status_bar: if self.status_bar_visible {
                self.status_bar_height()
            } else {
                0.
            },
            titlebar: self.titlebar_width,
        }
    }

    /// Text in a region starts as it does at the window's root.
    fn region_root(&self) -> Div {
        div()
            .size_full()
            .bg(self.fill().ground())
            .min_h_0()
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.config.ui)
            .text_size(px(self.config.ui.size))
    }

    pub(crate) fn render_region(
        &mut self,
        region: Region,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match region {
            Region::ActivityBar if self.activity_bar_leads() => self
                .region_root()
                .child(self.render_activity_bar(cx))
                .into_any_element(),
            // On the sidebar's fill, as the sidebar ends with it.
            Region::Devices if self.device_footer_shown() => self
                .region_root()
                .bg(self.fill().panel(self.theme.sidebar_background()))
                .child(self.render_device_footer(cx))
                .into_any_element(),
            Region::Header => self
                .region_root()
                .child(self.sidebar_header(window, cx))
                .into_any_element(),
            Region::Overlay => {
                // See-through: it only holds what draws over the window.
                let root = div().size_full().relative().flex().flex_col();
                let mut root = root.child(AnyView::from(self.mark_layer.clone()));
                self.append_overlays(&mut root, window, cx);
                root.into_any_element()
            }
            Region::Sidebar => self.render_sidebar_region(window, cx),
            Region::StatusBar if self.status_bar_visible => self
                .region_root()
                .child(self.render_status_bar(cx))
                .into_any_element(),
            Region::TitleBar => self.render_titlebar_region(window, cx),
            _ => Empty.into_any_element(),
        }
    }

    /// The title bar's end at its own width, which it reports so the shell
    /// can size the region and the rightmost strip can leave it free.
    fn render_titlebar_region(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().downgrade();
        let end = self.titlebar_end(window, cx).relative().child(
            gpui::canvas(
                move |bounds, _, cx| {
                    let width = f32::from(bounds.size.width).ceil();
                    let _ = view.update(cx, |this, cx| {
                        if this.titlebar_width != width {
                            this.titlebar_width = width;
                            cx.notify();
                        }
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        );
        self.region_root()
            .bg(self.fill().panel(self.theme.surface))
            .flex()
            .items_center()
            .justify_end()
            .child(end)
            .into_any_element()
    }

    fn render_sidebar_region(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.activity_section().is_some() || self.mod_app().is_some() {
            return Empty.into_any_element();
        }
        let indicators = self.sidebar_view.read(cx).indicators();
        let sidebar = match self.sidebar_mode() {
            SidebarMode::Expanded => self.render_sidebar(indicators, window, cx),
            SidebarMode::Rail => self.render_rail(indicators, window, cx),
            SidebarMode::Hidden => return Empty.into_any_element(),
        };
        self.region_root()
            .flex()
            .flex_col()
            .when(self.activity_bar_leads(), |panel| {
                panel.child(self.panel_title(ActivityPanel::Workspaces, cx))
            })
            .child(div().flex().flex_1().min_h_0().child(sidebar))
            .into_any_element()
    }

    /// What draws over the window, last so it covers the rest of `root`.
    /// Append in place: moving a full `Div` through a builder chain keeps
    /// extra stack slots live while a dialog renders on Windows' UI thread.
    pub(super) fn append_overlays(
        &mut self,
        root: &mut impl ParentElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        root.extend(self.render_tab_ghost(cx));
        root.extend(self.render_core_note(cx).map(IntoElement::into_any_element));
        root.extend(self.render_toasts(window, cx));
        root.extend(self.render_file_transfer(window, cx));
        root.extend(self.render_menu_leaving(window, cx));
        // The layout draws the device picker itself.
        if self.menu.page.is_some() && !self.hosted_picker_open() {
            root.extend([self.render_menu(window, cx).into_any_element()]);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
