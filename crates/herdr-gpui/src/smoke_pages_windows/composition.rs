//! A deterministic check of the parent's native composition layer. Windows
//! permits only one DirectComposition target per HWND per layer. Taking the
//! topmost slot must succeed: GPUI's full-window surface belongs below child
//! HWNDs, otherwise it covers WebView2 even when the page loads and is shown.
//!
//! No safe API exposes this slot. The temporary target has no visual tree,
//! is never committed, and is released immediately; this does not alter the
//! window's drawing or WebView2's lifecycle.
#![allow(unsafe_code)]

use anyhow::{Context as _, Result};
use gpui::Window;
use windows::Win32::{
    Foundation::{DCOMPOSITION_ERROR_WINDOW_ALREADY_COMPOSED, HWND},
    Graphics::DirectComposition::{DCompositionCreateDevice2, IDCompositionDesktopDevice},
};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(super) fn check(window: &Window) -> Result<()> {
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)?.as_raw() else {
        anyhow::bail!("not a Win32 window");
    };
    // SAFETY: a null rendering device is supported by DCompositionCreateDevice2
    // when no surfaces are created. COM is initialized by the native platform;
    // this runs on its UI thread, and all interfaces stay on that thread.
    let device: IDCompositionDesktopDevice = unsafe { DCompositionCreateDevice2(None) }
        .context("create the composition inspection device")?;
    // SAFETY: the HWND belongs to the borrowed, open fixture window on this
    // thread. This takes only an unused slot, never an existing target.
    match unsafe { device.CreateTargetForHwnd(HWND(handle.hwnd.get() as _), true) } {
        Ok(target) => {
            drop(target);
            Ok(())
        }
        Err(error) if error.code() == DCOMPOSITION_ERROR_WINDOW_ALREADY_COMPOSED => Err(error)
            .context(
                "the parent owns the composition layer above child HWNDs, covering WebView2 (#826)",
            ),
        Err(error) => Err(error).context("inspect the parent's topmost composition slot"),
    }
}
