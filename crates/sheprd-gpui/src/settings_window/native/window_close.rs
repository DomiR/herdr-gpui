//! Exercise AppKit's close button with the same KVO registration Touch Bar uses.
#![allow(unsafe_code)]

use super::super::themes::{Choice, Scope};
use super::*;
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSView, NSWindow};
use objc2_foundation::{NSObject, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub(crate) async fn verify_native_close(
    main: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let original = open_settings(main, cx)?;
    close_observed(original.into(), cx, press_close_button).await?;

    let reopened = open_settings(main, cx)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while reopened.update(cx, |view, _, _| view.busy())? {
        ensure!(Instant::now() < deadline, "reopened Settings did not load");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    let theme = "Nord"; // i18n: data
    reopened.update(cx, |view, _, cx| {
        view.accept_theme_choice(
            Choice {
                scope: Scope::App,
                name: theme.into(),
            },
            cx,
        );
    })?;
    close_observed(reopened.into(), cx, |_, cx| {
        AnyWindowHandle::from(reopened).update(cx, |_, window, cx| -> Result<()> {
            window.draw(cx).clear(cx);
            ensure!(
                window.dispatch_keystroke(Keystroke::parse("cmd-w")?, cx),
                "Settings did not handle Cmd-W"
            );
            Ok(())
        })?
    })
    .await?;
    cx.background_executor()
        .spawn(async move {
            let text = std::fs::read_to_string(Config::local_path()?)?;
            let config: toml::Table = toml::from_str(&text)?;
            ensure!(
                config.get("theme").and_then(toml::Value::as_str) == Some(theme),
                "closing Settings did not save the theme"
            );
            anyhow::Ok(())
        })
        .await?;

    let other = cx.update(|cx| {
        crate::app::open_window(
            sheprd_client::ConnectTarget::Local,
            crate::updater::Updater::default(),
            cx,
            true,
        )
    })?;
    close_observed(other.into(), cx, press_close_button).await?;
    let last = main.update(cx, |_, window, _| native_window(window))??;
    eprintln!(
        // i18n: data
        "WINDOW CLOSE native PASS: Settings closed twice, Cmd-W saved the theme, and another main window closed"
    );
    // Closing the last window exercises the app's normal quit path. The outer
    // test checks the process's exit status.
    last.performClose(None);
    Ok(())
}

fn open_settings(
    main: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
) -> Result<WindowHandle<SettingsWindow>> {
    main.update(cx, |_, _, cx| open(cx.entity().downgrade(), cx))?;
    cx.update(|cx| {
        cx.windows()
            .into_iter()
            .find_map(|window| window.downcast::<SettingsWindow>())
    })
    .context("Settings did not open")
}

fn press_close_button(window: &NSWindow, _: &mut AsyncApp) -> Result<()> {
    window.performClose(None);
    Ok(())
}

async fn close_observed(
    handle: AnyWindowHandle,
    cx: &mut AsyncApp,
    close: impl FnOnce(&NSWindow, &mut AsyncApp) -> Result<()>,
) -> Result<()> {
    let window = handle.update(cx, |_, window, _| native_window(window))??;
    let view = window
        .contentView()
        .context("missing native content view")?;
    let observer = NSObject::new();
    let key_path = NSString::from_str("nextResponder");
    let object: &AnyObject = &view;
    let original_class = object.class();
    // SAFETY: AppKit owns this live view. The observer and key path stay alive
    // until removal, and both operations run on AppKit's thread.
    unsafe {
        let _: () = msg_send![object, addObserver: &*observer, forKeyPath: &*key_path,
            options: 0usize, context: std::ptr::null_mut::<std::ffi::c_void>()];
    }
    let observed_class = object.class();
    ensure!(
        observed_class
            .name()
            .to_bytes()
            .starts_with(b"NSKVONotifying_AccessKitSubclassOf"),
        "expected a KVO subclass over the active AccessKit adapter"
    );

    // Native close reenters GPUI, so perform it outside the update above.
    close(&window, cx)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while cx.update(|cx| cx.windows().contains(&handle)) {
        ensure!(Instant::now() < deadline, "native window did not close");
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    ensure!(
        std::ptr::eq(object.class(), observed_class),
        "closing the window discarded AppKit's KVO subclass (previous class: {})",
        original_class.name().to_string_lossy()
    );
    // SAFETY: This removes exactly the registration above. The retained view
    // lets AppKit clean up after GPUI has dropped the adapter.
    unsafe {
        let _: () = msg_send![object, removeObserver: &*observer, forKeyPath: &*key_path];
    }
    Ok(())
}

fn native_window(window: &Window) -> Result<Retained<NSWindow>> {
    let handle =
        HasWindowHandle::window_handle(window).map_err(|error| anyhow::anyhow!("{error}"))?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        bail!("fixture did not open an AppKit window");
    };
    // SAFETY: GPUI's borrowed AppKit handle supplies a live NSView on the UI
    // thread. NSView::window retains the returned window across the update.
    let view = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
    view.window().context("fixture view has no window")
}
