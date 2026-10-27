//! Keeping the window's view of the daemon current: resending geometry only
//! when it actually changed, renaming the OS window after the focused space,
//! and reporting focus to the authoritative inbox as well as the wire.

use super::HerdrWindow;
use crate::{WINDOW_TITLE, sidebar, state::LiveState};
use gpui::{Context, Pixels, Point, Window};
use std::time::{Duration, Instant};

/// A drag or the full screen animation passes through many sizes, and each
/// resize makes every pane's program redraw, agents re-rendering whole
/// transcripts. Only the size the window settles on is sent.
pub(crate) const RESIZE_SETTLE: Duration = Duration::from_millis(150);

/// Retry geometry metadata more slowly than a settled window resize, so the
/// request is not resent while the daemon answers it. This never claims focus.
pub(crate) const RESIZE_REASSERT: Duration = Duration::from_secs(1);

/// How long after input to keep reporting geometry if the frame disagrees.
/// The daemon may retain another client's size; only new input claims it.
pub(crate) const REASSERT_AFTER_INPUT: Duration = Duration::from_secs(5);

const BELL_PREVIEW_DELAY: Duration = Duration::from_secs(3);

/// Whether an active window may retry its geometry metadata at `now`.
fn reasserts(last_input: Option<Instant>, now: Instant) -> bool {
    last_input.is_some_and(|last| now.saturating_duration_since(last) < REASSERT_AFTER_INPUT)
}

impl HerdrWindow {
    /// Records that the window just forwarded user input.
    pub(crate) fn note_input(&mut self) {
        self.last_input = Some(Instant::now());
        // A size mismatch must not turn a poll into session navigation. Only
        // actual input may claim the displayed tab back. Use the normal focus
        // fence so further input waits for the claim instead of queuing more
        // claims against an old projection.
        if !self.active
            || !self.surface_activation_ready()
            || self.surface_matches_options()
            || self.activation_deadline.is_some()
            || self.pending_navigation.is_some()
            || self.pending_toast.is_some()
        {
            return;
        }
        let Some(tab) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_tab_id.clone())
        else {
            return;
        };
        let options = self.options;
        if self.request_focus_change(
            "Focus terminal",
            Some(crate::NavigationTarget::Tab(tab.clone())),
            |handle, boot| {
                handle.resize(boot, options)?;
                handle.focus_tab(boot, &tab)
            },
        ) {
            self.last_queued_options = Some(options);
            self.pending_input.geometry_claim = true;
        }
    }

    /// Records the pointer at `position` over the panes as use only when it
    /// moved, and returns whether it did: platforms such as Windows report
    /// move events for a cursor that did not move, and a resting cursor must
    /// not keep an idle window in use.
    pub(crate) fn note_pointer(&mut self, position: Point<Pixels>) -> bool {
        if self.last_pointer == Some(position) {
            return false;
        }
        self.last_pointer = Some(position);
        // Hover is useful for sizing, but is not an explicit tab selection.
        self.last_input = Some(Instant::now());
        true
    }

    pub(crate) fn resize(&mut self) {
        self.resize_at(Instant::now());
    }

    pub(crate) fn resize_at(&mut self, now: Instant) {
        // A background window must not compete for geometry, even immediately
        // after input. Endpoint activation sends its initial size itself.
        if !self.active || !self.endpoints[self.selected_endpoint].surface_requested() {
            self.pending_resize = None;
            return;
        }
        // Another client (a CLI, or another window) can resize the tab the
        // window shows. The window's own options still hold, so the early
        // return below would leave a frame that no longer matches them. An
        // idle window leaves a frame another client sized alone; only one
        // used within `REASSERT_AFTER_INPUT` asks for its size again.
        let stale = self.live.surface.is_some()
            && !self.surface_matches_options()
            && reasserts(self.last_input, now);
        let changed = self.last_queued_options != Some(self.options);
        if self.last_queued_options == Some(self.options) && !stale {
            self.pending_resize = None;
            return;
        }
        // The first geometry goes out at once; later changes wait to settle.
        if self.last_queued_options.is_some() {
            match self.pending_resize {
                Some((options, since)) if options == self.options => {
                    let wait = if changed {
                        RESIZE_SETTLE
                    } else {
                        RESIZE_REASSERT
                    };
                    if now.duration_since(since) < wait {
                        return;
                    }
                }
                _ => {
                    self.pending_resize = Some((self.options, now));
                    return;
                }
            }
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) {
            match handle.resize(&snapshot.boot_id, self.options) {
                Ok(()) => {
                    self.last_queued_options = Some(self.options);
                    self.pending_resize = Some((self.options, now));
                    // Geometry reconciliation never selects a tab or
                    // re-reports focus. Explicit input owns that claim.
                }
                // Backpressure is transient: the options stay unqueued, so the
                // next poll or paint retries them once the worker drains. A
                // status error would outlive the retry that already fixed it.
                Err(herdr_client::Error::Full) => {}
                Err(error) => self.local_error = Some(format!("Resize: {error}")),
            }
        }
    }

    /// Rings the bell the selected connection forwarded, as `[bell]` asks.
    pub(crate) fn ring_bell(&mut self, window: &mut Window) {
        let Some(ring) =
            self.bell
                .take(self.config.bell, window.is_window_active(), Instant::now())
        else {
            return;
        };
        ring_bell(window, ring);
    }

    /// QA > Ring Bell: both reactions, whatever `[bell]` says, after a delay
    /// long enough to switch to another app, since attention is only asked
    /// for while the window is inactive.
    pub(crate) fn preview_bell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(BELL_PREVIEW_DELAY).await;
            let _ = this.update_in(cx, |_, window, _| {
                ring_bell(
                    window,
                    crate::bell::Ring {
                        attention: !window.is_window_active(),
                        sound: true,
                    },
                );
            });
        })
        .detach();
    }

    /// The daemon's title for the selected connection wins: an agent's
    /// `client.window_title.set` or Herdr's rendered `ui.window_title`.
    /// Otherwise macOS lists every window in the Window menu by title, and
    /// windows onto the same daemon are told apart by the space each shows.
    pub(crate) fn sync_window_title(&mut self, window: &mut Window) {
        let title = window_title(&self.live);
        if self.title != title {
            window.set_window_title(&title);
            self.title = title;
        }
    }

    pub(crate) fn report_focus(&mut self) {
        // The first positive report acknowledges the daemon's active tab. Wait
        // until its surface is coherent, but not necessarily our size: focus
        // claims tab geometry, so waiting for that size can deadlock startup.
        // Do not flap focus on later frame gaps.
        let focused = self.active
            && self.endpoints[self.selected_endpoint].surface_requested()
            && (self.sent_focus == Some(true)
                || (self.pending_toast.is_none() && self.surface_activation_ready()));
        // Update the authoritative event inbox, not just the rendered clone.
        if let Ok(mut state) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .try_lock()
        {
            state.set_outer_focus(self.active && self.input_ready());
        }
        if self.sent_focus == Some(focused) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) && handle.set_focus(&snapshot.boot_id, focused).is_ok()
        {
            self.sent_focus = Some(focused);
        }
    }
}

fn ring_bell(window: &Window, ring: crate::bell::Ring) {
    if ring.attention {
        window.request_attention();
    }
    if ring.sound {
        window.play_system_bell();
    }
}

fn window_title(live: &LiveState) -> String {
    if let Some(title) = &live.window_title {
        return title.clone();
    }
    live.snapshot
        .as_ref()
        .and_then(|snapshot| {
            let focused = snapshot.focused_workspace_id.as_deref()?;
            snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == focused)
        })
        .map_or_else(
            || WINDOW_TITLE.to_owned(),
            |workspace| {
                format!(
                    "{WINDOW_TITLE} \u{2014} {}",
                    sidebar::workspace_label(workspace, false)
                )
            },
        )
}
