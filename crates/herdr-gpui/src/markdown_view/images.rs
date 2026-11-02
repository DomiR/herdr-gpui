//! Images that Markdown bodies show, read from the web on the background
//! executor and kept for the app's life within a byte budget. Views ask for
//! a body's images when they read or open it, never while drawing; a
//! finished read redraws the windows. Only `https` addresses are read, and
//! plain `http` only from this machine (herdr-dog's attachments). No
//! credentials are ever sent: Linear's files arrive signed.

use gpui::{App, Global, Image, ImageFormat};
use std::{collections::HashMap, io::Cursor, sync::Arc, time::Duration};

/// The largest image read.
const MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Images kept at once, by encoded size; past it the cache starts over.
const BUDGET: usize = 128 * 1024 * 1024;
/// The most images asked for at once.
const MAX_LOADING: usize = 16;
const MAX_SIDE: u32 = 8192;

enum Entry {
    Loading,
    Ready(Arc<Image>),
    Failed,
}

#[derive(Default)]
pub(crate) struct RemoteImages {
    entries: HashMap<String, Entry>,
    bytes: usize,
}

impl Global for RemoteImages {}

/// What an address is cached under: a signed Linear file's address without
/// its signature, which every sync renews.
fn key(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) if parsed.host_str() == Some("uploads.linear.app") => {
            parsed.set_query(None);
            parsed.to_string()
        }
        _ => url.to_owned(),
    }
}

/// Whether `url` may be read: `https`, or `http` on a loopback host.
pub(crate) fn readable(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    match parsed.scheme() {
        "https" => parsed.host().is_some(),
        "http" => match parsed.host() {
            Some(url::Host::Domain(host)) => host == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

/// The image at `url`, once read.
pub(crate) fn image(url: &str, cx: &App) -> Option<Arc<Image>> {
    match cx.try_global::<RemoteImages>()?.entries.get(&key(url))? {
        Entry::Ready(image) => Some(image.clone()),
        Entry::Loading | Entry::Failed => None,
    }
}

/// Starts reading each of `urls` not read or being read yet.
pub(crate) fn request<'a>(urls: impl IntoIterator<Item = &'a str>, cx: &mut App) {
    if cx.try_global::<RemoteImages>().is_none() {
        cx.set_global(RemoteImages::default());
    }
    for url in urls {
        if !readable(url) {
            continue;
        }
        let key = key(url);
        let images = cx.global_mut::<RemoteImages>();
        let loading = images
            .entries
            .values()
            .filter(|entry| matches!(entry, Entry::Loading))
            .count();
        if images.entries.contains_key(&key) || loading >= MAX_LOADING {
            continue;
        }
        images.entries.insert(key.clone(), Entry::Loading);
        let url = url.to_owned();
        let read = cx
            .background_executor()
            .spawn(async move { download(&url) });
        cx.spawn(async move |cx| {
            let image = read.await;
            cx.update(|cx| {
                let images = cx.global_mut::<RemoteImages>();
                let entry = match image {
                    Some(image) => {
                        images.bytes += image.bytes.len();
                        if images.bytes > BUDGET {
                            // Starting over is simpler than tracking use, and
                            // the next look reads what still shows.
                            images
                                .entries
                                .retain(|_, entry| matches!(entry, Entry::Loading));
                            images.bytes = image.bytes.len();
                        }
                        Entry::Ready(image)
                    }
                    None => Entry::Failed,
                };
                images.entries.insert(key, entry);
                cx.refresh_windows();
            })
        })
        .detach();
    }
}

fn download(url: &str) -> Option<Arc<Image>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(3)
        .build()
        .into();
    let mut response = agent.get(url).call().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .ok()?;
    decode(bytes)
}

/// `bytes` as an image GPUI draws, when they are a PNG, JPEG, GIF, or WebP
/// of a sane size.
fn decode(bytes: Vec<u8>) -> Option<Arc<Image>> {
    let format = image::guess_format(&bytes).ok()?;
    let gpui_format = match format {
        image::ImageFormat::Png => ImageFormat::Png,
        image::ImageFormat::Jpeg => ImageFormat::Jpeg,
        image::ImageFormat::Gif => ImageFormat::Gif,
        image::ImageFormat::WebP => ImageFormat::Webp,
        _ => return None,
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    // Decoding once proves the bytes are the image they claim.
    reader.decode().ok()?;
    Some(Arc::new(Image::from_bytes(gpui_format, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_machines_http_is_read() {
        assert!(readable("https://uploads.linear.app/a/b.png"));
        assert!(readable("http://127.0.0.1:4319/api/attachments/1/a.png"));
        assert!(readable("http://localhost:4319/a.png"));
        assert!(!readable("http://example.com/a.png"));
        assert!(!readable("file:///etc/passwd"));
        assert!(!readable("https://user:pass@example.com/a.png"));
        assert!(!readable("not a url"));
    }

    #[test]
    fn a_signed_linear_file_is_cached_without_its_signature() {
        assert_eq!(
            key("https://uploads.linear.app/w/f.png?signature=1&expires=2"),
            "https://uploads.linear.app/w/f.png"
        );
        assert_eq!(
            key("https://example.com/a.png?v=2"),
            "https://example.com/a.png?v=2"
        );
    }
}
