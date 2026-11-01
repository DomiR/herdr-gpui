//! Markdown drawn natively, as the issue and ticket pages show bodies:
//! headings, emphasis, code, links, quotes, lists, task lists, tables,
//! rules, and images. `parse` reads a body once, off the UI thread where a
//! view can; `render` draws the blocks; `images` reads the images they
//! show, which a view asks for when it reads or opens a body.

mod images;
mod parse;
mod render;

#[cfg(test)]
mod tests;

pub(crate) use images::{image as remote_image, request as request_images};
pub(crate) use parse::{Block, images as image_urls, parse};
pub(crate) use render::{block, render};

/// Makes every relative image address in `blocks` absolute against `base`,
/// as herdr-dog's attachments are written (`/api/attachments/...`).
pub(crate) fn resolve_images(blocks: &mut [Block], base: &url::Url) {
    for block in blocks {
        match block {
            Block::Image { url, .. } => {
                if url::Url::parse(url).is_err()
                    && let Ok(joined) = base.join(url)
                {
                    *url = joined.to_string();
                }
            }
            Block::Quote(inner) => resolve_images(inner, base),
            Block::List { items, .. } => {
                for item in items {
                    resolve_images(&mut item.blocks, base);
                }
            }
            _ => {}
        }
    }
}
