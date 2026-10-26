//! Linear's own links open in the Linear app when it is installed: its
//! `linear://` addresses mirror the web ones, so `https://linear.app/acme/
//! issue/ENG-12` is `linear://acme/issue/ENG-12`. On macOS `open` says
//! whether an app took the address, run off the UI thread; when none did,
//! the web address opens in the system browser. Elsewhere there is no such
//! answer to wait for, so the system browser opens it.

use gpui::App;

/// The Linear app's address for `url`, when `url` is a Linear web page.
pub(crate) fn app_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("linear.app") {
        return None;
    }
    let path = parsed.path().trim_start_matches('/');
    if path.is_empty() {
        return None;
    }
    let mut app = format!("linear://{path}");
    if let Some(query) = parsed.query() {
        app.push('?');
        app.push_str(query);
    }
    if let Some(fragment) = parsed.fragment() {
        app.push('#');
        app.push_str(fragment);
    }
    Some(app)
}

/// Opens `url` in the Linear app, else the system browser, when it is a
/// Linear link; whether it was one.
pub(crate) fn open_link(url: &str, cx: &mut App) -> bool {
    let Some(app) = app_url(url) else {
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

#[cfg(test)]
mod tests {
    use super::app_url;

    #[test]
    fn linear_pages_map_to_the_apps_addresses() {
        assert_eq!(
            app_url("https://linear.app/acme/issue/ENG-12/fix-login").as_deref(),
            Some("linear://acme/issue/ENG-12/fix-login")
        );
        assert_eq!(
            app_url("https://linear.app/acme/issue/ENG-12#comment-1").as_deref(),
            Some("linear://acme/issue/ENG-12#comment-1")
        );
        assert_eq!(app_url("https://linear.app/").as_deref(), None);
        assert_eq!(app_url("https://github.com/acme/repo/issues/1"), None);
        assert_eq!(app_url("http://linear.app/acme/issue/ENG-1"), None);
        assert_eq!(app_url("https://uploads.linear.app/a/b.png"), None);
    }
}
