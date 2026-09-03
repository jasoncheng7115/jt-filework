//! Editing a file as bytes.
//!
//! The read-only hex window in `jtf-viewer` formats rows of a file into
//! strings. That is the right shape for looking and the wrong one for
//! changing, so editing lives here instead of growing out of it: an edit
//! buffer that never loads the file, undo that never snapshots it, and search
//! that reads it a window at a time.
//!
//! Everything in this crate is about bytes and offsets. Nothing in it knows
//! about a window, a font or a keystroke — the Qt layer asks it questions and
//! draws the answers, which is what lets all of it be tested without one.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod buffer;
pub mod clip;
pub mod find;
pub mod goto;
pub mod history;
pub mod session;

pub use buffer::{Buffer, Byte};
pub use clip::{parse_paste, render, Format, Pasted};
pub use find::{find_backward, find_forward, Kind, Needle, Width};
pub use goto::resolve;
pub use history::History;
pub use session::{Bookmark, Selection, Session};
