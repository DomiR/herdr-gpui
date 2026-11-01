//! Linear's own links open in the Linear app when it is installed, at the
//! address [`super::linear::app_url`] gives. On macOS `open` says whether an
//! app took the address, run off the UI thread; when none did, the web
//! address opens in the system browser. Elsewhere there is no such answer to
//! wait for, so the system browser opens it.

use gpui::App;

/// Opens `url` in the Linear app, else the system browser, when it is a
/// Linear link; whether it was one.
pub(crate) fn open_link(url: &str, cx: &mut App) -> bool {
    let Some(app) = super::linear::app_url(url) else {
        return false;
    };
    #[cfg(target_os = "macos")]
    {
        let web = url.to_owned();
        let opened = cx.background_executor().spawn(async move {
            herdr_client::new_command("open")
                .arg(&app)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        });
        cx.spawn(async move |cx| {
            if !opened.await {
                cx.update(|cx| cx.open_url(&web));
            }
        })
        .detach();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        cx.open_url(url);
    }
    true
}
