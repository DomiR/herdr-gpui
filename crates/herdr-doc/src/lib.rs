//! Markdown documents several writers change at once: the person in Herdr
//! UI's editor, agents through `herdr-gpui doc` commands, and agents (or
//! any program) rewriting the file on disk. Each open file is one Loro CRDT
//! text, so concurrent edits merge instead of overwriting each other; the
//! file stays plain Markdown and remains the source of truth.
//!
//! - [`Document`]: the CRDT text, the user's edits and undo, agents' edits
//!   as their own peers, and merging a new disk text against the version
//!   last synced with the disk.
//! - [`disk`]: bounded reads and atomic writes of the file.
//! - [`diff`]: the edits that turn one text into another.
//! - [`markdown`]: each line's block kind and inline styles, for drawing.
//! - [`Version`]: the hash agents name their changes against, and the
//!   diff a stale one gets back.
//! - [`section`]: the sections headings start, by slug.
//! - [`todo`]: task lists: listing, adding, checking, moving, and indenting
//!   items, as edits.
//!
//! No GPUI and no I/O outside [`disk`]; callers run disk work off their UI
//! thread.

pub mod diff;
pub mod disk;
mod document;
mod edit;
mod error;
pub mod markdown;
pub mod section;
pub mod todo;
mod version;

pub use document::{Anchor, Applied, Author, Document, Merge, Scope, Synced};
pub use edit::Edit;
pub use error::{Error, Result};
pub use version::{Conflict, MAX_DIFF, Version};

/// The largest document, in bytes, read, edited, or written.
pub const MAX_BYTES: usize = 2 * 1024 * 1024;

#[cfg(test)]
mod tests;
