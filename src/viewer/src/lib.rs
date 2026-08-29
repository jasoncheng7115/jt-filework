//! Looking at a file's contents.
//!
//! `AGENTS.md` §14 separates Preview from Viewer; this crate serves the
//! Viewer: stateful, richer, and expected to open things that do not fit in
//! memory.
//!
//! # The rule that shapes everything here
//!
//! **Nothing loads a whole file.** A view is a window onto a byte range, and
//! the window has a hard size. A 10 GB log opens as fast as a 10 KB one
//! because the work is proportional to what is on screen, not to what is on
//! disk (`docs/VIEWER_PREVIEW.md` §4.1).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod detect;
mod hex;
mod text;

pub use detect::{detect, ContentKind};
pub use hex::{HexView, HexWindow, ROW_BYTES};
pub use text::{Encoding, LineEnding, TextView, TextWindow};
