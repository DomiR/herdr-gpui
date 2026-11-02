//! Linear's own links open in the Linear app when it is installed: its
//! `linear://` addresses mirror the web ones, so `https://linear.app/acme/
//! issue/ENG-12` is `linear://acme/issue/ENG-12`. The app opens them.

/// The Linear app's address for `url`, when `url` is a Linear web page.
pub fn app_url(url: &str) -> Option<String> {
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
