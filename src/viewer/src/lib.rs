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

mod archive;
mod detect;
mod hex;
pub mod iso;
mod text;

pub use archive::{
    list as list_archive, ArchiveEntry, MAX_DIRECTORY_BYTES, MAX_ENTRIES, MAX_NAME_BYTES,
};
pub use detect::{detect, ContentKind};
pub use iso::{is_image as is_iso, list as list_iso, read as read_iso, Extent, IsoEntry};

/// What kind of container a file is, if it is one this build can open.
///
/// One place decides, so the archive window, the `Z` key, the preview and the
/// pane's own "can I navigate into this" cannot disagree about what counts as
/// a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// A ZIP archive.
    Zip,
    /// An ISO 9660 disc image (ADR-0005).
    Iso,
    /// A tar, on its own or inside gzip, bzip2 or xz (ADR-0006).
    Tar,
}

/// Which container `path` is, or `None` for anything else.
///
/// Decided by the file's own bytes, not by its name: a `.zip` that is not one
/// should fail to open rather than open into an empty window, and an image
/// named `.img` is still an image.
#[must_use]
pub fn container_of(path: &std::path::Path) -> Option<Container> {
    match detect(path) {
        // `Archive` covers ZIP and the three compressed-tar spellings, which
        // are told apart by their own signatures rather than by extension.
        Ok(ContentKind::Archive) => Some(if is_zip(path) {
            Container::Zip
        } else {
            Container::Tar
        }),
        Ok(ContentKind::DiskImage) => Some(Container::Iso),
        _ => None,
    }
}

/// Whether the file starts with a local file header, which is what makes a
/// ZIP a ZIP.
fn is_zip(path: &std::path::Path) -> bool {
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0_u8; 4];
    file.read_exact(&mut head).is_ok() && head == [b'P', b'K', 0x03, 0x04]
}

/// Every entry in the container at `path`.
///
/// # Errors
///
/// Whatever the reader for that container reports, and
/// [`jtf_core::ErrorCode::Unsupported`] when the file is not one this build
/// can open.
pub fn list_container(path: &std::path::Path) -> Result<Vec<ArchiveEntry>, jtf_core::Error> {
    match container_of(path) {
        Some(Container::Zip) => list_archive(path),
        Some(Container::Iso) => list_iso(path),
        // Listing a tar is `jtf-fs`'s, because reading one means running the
        // decompressor and that is where the decompressors live. The viewer
        // reports the kind; the caller asks the right reader.
        Some(Container::Tar) => Err(jtf_core::Error::new(
            jtf_core::ErrorCode::Unsupported,
            "a tar is listed through jtf-fs",
        )),
        None => Err(jtf_core::Error::new(
            jtf_core::ErrorCode::Unsupported,
            "not a container this build can open",
        )),
    }
}
pub use hex::{HexView, HexWindow, ROW_BYTES};
pub use text::{
    Encoding, LineEnding, TextView, TextWindow, MAX_INDEXED_BYTES, PREVIEW_INDEXED_BYTES,
};
