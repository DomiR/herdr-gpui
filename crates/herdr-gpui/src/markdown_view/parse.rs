//! Markdown into the blocks the native views draw. Bodies come from
//! trackers and agents, so they are untrusted: the document is bounded in
//! size and blocks, raw HTML is dropped but for an `<img>`'s address, and
//! links keep only web and mail addresses.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// The most of a body read; past it the rest is dropped.
const MAX_SOURCE: usize = 256 * 1024;
/// The most blocks a body becomes, nested ones included.
const MAX_BLOCKS: usize = 4096;
/// The longest address kept.
const MAX_URL: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Style {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) code: bool,
}

/// A run of styled text, as `Inline::text`'s byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) range: Range<usize>,
    pub(crate) style: Style,
    /// The address the span links to.
    pub(crate) link: Option<String>,
}

/// Text with its styled runs; unstyled text has no span.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Inline {
    pub(crate) text: String,
    pub(crate) spans: Vec<Span>,
}

impl Inline {
    pub(crate) fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListItem {
    /// `Some(done)` for a task list item.
    pub(crate) task: Option<bool>,
    pub(crate) blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Paragraph(Inline),
    Heading(u8, Inline),
    Code(String),
    Quote(Vec<Block>),
    /// `start` is the first number of an ordered list.
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Rule,
    Image {
        url: String,
        alt: String,
    },
    Table {
        head: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
    },
}

/// Every image address in `blocks`, nested ones included.
pub(crate) fn images(blocks: &[Block]) -> Vec<&str> {
    let mut found = Vec::new();
    for block in blocks {
        match block {
            Block::Image { url, .. } => found.push(url.as_str()),
            Block::Quote(inner) => found.extend(images(inner)),
            Block::List { items, .. } => {
                for item in items {
                    found.extend(images(&item.blocks));
                }
            }
            _ => {}
        }
    }
    found
}

/// What a container being read collects.
enum Open {
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item(ListItem),
    Table {
        head: Vec<Inline>,
        rows: Vec<Vec<Inline>>,
        row: Vec<Inline>,
        in_head: bool,
    },
}

struct Reader {
    /// Finished top-level blocks.
    done: Vec<Block>,
    open: Vec<Open>,
    inline: Inline,
    style: Style,
    link: Option<String>,
    /// The heading level while one is read.
    heading: Option<u8>,
    code: Option<String>,
    /// An image being read: its address and the alt text so far.
    image: Option<(String, String)>,
    count: usize,
}

/// `source` as blocks.
pub(crate) fn parse(source: &str) -> Vec<Block> {
    let source = match source.char_indices().nth(MAX_SOURCE) {
        Some((cut, _)) => &source[..cut],
        None => source,
    };
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut reader = Reader {
        done: Vec::new(),
        open: Vec::new(),
        inline: Inline::default(),
        style: Style::default(),
        link: None,
        heading: None,
        code: None,
        image: None,
        count: 0,
    };
    for event in Parser::new_ext(source, options) {
        if reader.count >= MAX_BLOCKS {
            break;
        }
        reader.event(event);
    }
    reader.flush();
    while let Some(open) = reader.open.pop() {
        reader.close(open);
    }
    reader.done
}

impl Reader {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(text) => {
                let style = Style {
                    code: true,
                    ..self.style
                };
                self.push(&text, style);
            }
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.text("\n"),
            Event::Rule => {
                self.flush();
                self.add(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                if let Some(Open::Item(item)) = self.open.last_mut() {
                    item.task = Some(done);
                }
            }
            // Raw HTML draws nothing but the image it names.
            Event::Html(html) | Event::InlineHtml(html) => {
                if let Some(url) = img_src(&html) {
                    self.flush();
                    self.add(Block::Image {
                        url,
                        alt: String::new(),
                    });
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.flush(),
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                });
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.open.push(Open::Quote(Vec::new()));
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(start) => {
                self.flush();
                self.open.push(Open::List {
                    start,
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.flush();
                self.open.push(Open::Item(ListItem {
                    task: None,
                    blocks: Vec::new(),
                }));
            }
            Tag::Table(_) => {
                self.flush();
                self.open.push(Open::Table {
                    head: Vec::new(),
                    rows: Vec::new(),
                    row: Vec::new(),
                    in_head: false,
                });
            }
            Tag::TableHead => {
                if let Some(Open::Table { in_head, .. }) = self.open.last_mut() {
                    *in_head = true;
                }
            }
            Tag::TableCell => self.inline = Inline::default(),
            Tag::Emphasis => self.style.italic = true,
            Tag::Strong => self.style.bold = true,
            Tag::Strikethrough => self.style.strike = true,
            Tag::Link { dest_url, .. } => self.link = safe_link(&dest_url),
            Tag::Image { dest_url, .. } => {
                self.image = Some((dest_url.to_string(), String::new()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => self.flush(),
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.add(Block::Code(code.trim_end_matches('\n').to_owned()));
                }
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                if let Some(open @ Open::Quote(_)) = self.open.pop() {
                    self.close(open);
                }
            }
            TagEnd::List(_) => {
                self.flush();
                if let Some(open @ Open::List { .. }) = self.open.pop() {
                    self.close(open);
                }
            }
            TagEnd::Item => {
                self.flush();
                if let Some(open @ Open::Item(_)) = self.open.pop() {
                    self.close(open);
                }
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.inline);
                if let Some(Open::Table { row, .. }) = self.open.last_mut() {
                    row.push(cell);
                }
            }
            TagEnd::TableHead => {
                if let Some(Open::Table {
                    head, row, in_head, ..
                }) = self.open.last_mut()
                {
                    *head = std::mem::take(row);
                    *in_head = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(Open::Table { rows, row, .. }) = self.open.last_mut() {
                    rows.push(std::mem::take(row));
                }
            }
            TagEnd::Table => {
                if let Some(open @ Open::Table { .. }) = self.open.pop() {
                    self.close(open);
                }
            }
            TagEnd::Emphasis => self.style.italic = false,
            TagEnd::Strong => self.style.bold = false,
            TagEnd::Strikethrough => self.style.strike = false,
            TagEnd::Link => self.link = None,
            TagEnd::Image => {
                if let Some((url, alt)) = self.image.take() {
                    // An image stands as a block of its own, between the
                    // text before and after it.
                    self.flush();
                    if url.len() <= MAX_URL {
                        self.add(Block::Image { url, alt });
                    }
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(code) = &mut self.code {
            code.push_str(text);
        } else if let Some((_, alt)) = &mut self.image {
            alt.push_str(text);
        } else {
            self.push(text, self.style);
        }
    }

    fn push(&mut self, text: &str, style: Style) {
        let start = self.inline.text.len();
        self.inline.text.push_str(text);
        if style != Style::default() || self.link.is_some() {
            self.inline.spans.push(Span {
                range: start..self.inline.text.len(),
                style,
                link: self.link.clone(),
            });
        }
    }

    /// Ends the text read so far as a paragraph or heading.
    fn flush(&mut self) {
        let inline = std::mem::take(&mut self.inline);
        let heading = self.heading.take();
        if inline.is_empty() {
            return;
        }
        self.add(match heading {
            Some(level) => Block::Heading(level, inline),
            None => Block::Paragraph(inline),
        });
    }

    /// Adds `block` to the innermost open container, else to the document.
    fn add(&mut self, block: Block) {
        self.count += 1;
        match self.open.last_mut() {
            Some(Open::Quote(blocks)) => blocks.push(block),
            Some(Open::Item(item)) => item.blocks.push(block),
            // Text straight in a list or table, which Markdown does not
            // make, goes to the document.
            _ => self.done.push(block),
        }
    }

    fn close(&mut self, open: Open) {
        match open {
            Open::Quote(blocks) => self.add(Block::Quote(blocks)),
            Open::List { start, items } => self.add(Block::List { start, items }),
            Open::Item(item) => match self.open.last_mut() {
                Some(Open::List { items, .. }) => items.push(item),
                _ => {
                    for block in item.blocks {
                        self.add(block);
                    }
                }
            },
            Open::Table { head, rows, .. } => self.add(Block::Table { head, rows }),
        }
    }
}

/// A link's address, when it is a web or mail address.
fn safe_link(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    (url.len() <= MAX_URL
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|scheme| lower.starts_with(scheme))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control()))
    .then(|| url.to_owned())
}

/// The `src` of an `<img>` tag in `html`, if it has one.
fn img_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let tag = lower.find("<img")?;
    let after = &lower[tag..];
    let src = after.find("src=")? + tag + "src=".len();
    let rest = &html[src..];
    let (quote, rest) = match rest.chars().next()? {
        quote @ ('"' | '\'') => (quote, &rest[1..]),
        _ => (' ', rest),
    };
    let end = rest.find([quote, '>']).unwrap_or(rest.len());
    let url = rest[..end].trim();
    (!url.is_empty() && url.len() <= MAX_URL).then(|| url.to_owned())
}
