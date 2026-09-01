//! What a file actually is.
//!
//! `docs/VIEWER_PREVIEW.md` §1: extension alone never decides, and magic
//! bytes override a lying extension. A `.txt` full of ELF is not text, and
//! opening it as text is how an editor corrupts a binary.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use jtf_core::{Error, ErrorCode};

/// How many bytes are enough to decide.
const SNIFF: usize = 8192;

/// What a viewer should open the file as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentKind {
    /// Text, in some encoding.
    Text,
    /// A known image format.
    Image,
    /// A known archive format.
    Archive,
    /// A disc image: a filesystem in a file (ADR-0005).
    ///
    /// Its own kind rather than an archive, because it is not one - nothing in
    /// it is compressed, and calling it 「壓縮檔」 in the type column would be
    /// wrong in the one place a user goes to find out what something is.
    DiskImage,
    /// A PDF.
    Pdf,
    /// Anything else. Hex is always available and never wrong.
    Binary,
    /// The file is empty; there is nothing to decide.
    Empty,
}

impl ContentKind {
    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Text => "content.text",
            Self::Image => "content.image",
            Self::Archive => "content.archive",
            Self::DiskImage => "content.disk_image",
            Self::Pdf => "content.pdf",
            Self::Binary => "content.binary",
            Self::Empty => "content.empty",
        }
    }

    /// Whether a text viewer should be offered.
    pub const fn is_textual(self) -> bool {
        matches!(self, Self::Text | Self::Empty)
    }

    /// Every kind, for exhaustive tests and catalogue parity.
    pub const ALL: &'static [Self] = &[
        Self::Text,
        Self::Image,
        Self::Archive,
        Self::DiskImage,
        Self::Pdf,
        Self::Binary,
        Self::Empty,
    ];
}

/// Signatures, checked before anything else is guessed.
const SIGNATURES: &[(&[u8], ContentKind)] = &[
    (b"\x89PNG\r\n\x1a\n", ContentKind::Image),
    (b"\xff\xd8\xff", ContentKind::Image),
    (b"GIF87a", ContentKind::Image),
    (b"GIF89a", ContentKind::Image),
    (b"BM", ContentKind::Image),
    (b"%PDF-", ContentKind::Pdf),
    // Only what this build can actually open. `.7z` and `.rar` were listed
    // here and could not be read, so the type column called them 壓縮檔 and
    // pressing Enter opened a window that could list nothing - a label the
    // program cannot honour is worse than none (ADR-0006 condition 9).
    (b"PK\x03\x04", ContentKind::Archive),
    (b"\x1f\x8b", ContentKind::Archive),
    (b"BZh", ContentKind::Archive),
    (b"\xfd7zXZ\x00", ContentKind::Archive),
    (b"\x7fELF", ContentKind::Binary),
    (b"\xca\xfe\xba\xbe", ContentKind::Binary),
    (b"\xcf\xfa\xed\xfe", ContentKind::Binary),
];

/// Decide from the file's own bytes.
///
/// # Errors
///
/// Whatever the filesystem reports when the file cannot be read.
pub fn detect(path: &Path) -> Result<ContentKind, Error> {
    let mut file = File::open(path).map_err(|e| map_io(path, &e))?;
    let mut buffer = vec![0u8; SNIFF];
    let read = file.read(&mut buffer).map_err(|e| map_io(path, &e))?;
    buffer.truncate(read);
    let kind = classify(&buffer);
    // An ISO's own signature is at byte 32769 - sector 16, one byte in - which
    // is far past any prefix worth sniffing, so it cannot go in the table
    // above. Checked only for files that looked like nothing else, and only
    // for files long enough to hold it, so it costs one seek on the files it
    // might actually be.
    if kind == ContentKind::Binary && is_iso_9660(&mut file) {
        return Ok(ContentKind::DiskImage);
    }
    Ok(kind)
}

/// Whether `file` carries the ISO 9660 signature in its first volume
/// descriptor.
fn is_iso_9660(file: &mut File) -> bool {
    const SIGNATURE_AT: u64 = 16 * 2048 + 1;
    if file
        .metadata()
        .is_ok_and(|meta| meta.len() < SIGNATURE_AT + 5)
    {
        return false;
    }
    if file.seek(SeekFrom::Start(SIGNATURE_AT)).is_err() {
        return false;
    }
    let mut marker = [0_u8; 5];
    file.read_exact(&mut marker).is_ok() && &marker == b"CD001"
}

/// Decide from a prefix of the content. Split out so it can be fuzzed and
/// tested without touching a disk.
pub(crate) fn classify(bytes: &[u8]) -> ContentKind {
    if bytes.is_empty() {
        return ContentKind::Empty;
    }
    for (signature, kind) in SIGNATURES {
        if bytes.starts_with(signature) {
            return *kind;
        }
    }
    if looks_like_text(bytes) {
        ContentKind::Text
    } else {
        ContentKind::Binary
    }
}

/// Whether a byte range reads as text.
///
/// A NUL byte is the strongest signal there is: no text encoding this project
/// supports puts one in the middle of a line, and UTF-16's NULs are excluded
/// by checking for its byte-order mark first.
fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        return true; // UTF-16 with a BOM
    }
    if bytes.contains(&0) {
        return false;
    }
    // Control characters other than tab, newline, carriage return and form
    // feed do not appear in text. A few is a corrupted file; many is binary.
    let suspicious = bytes
        .iter()
        .filter(|byte| **byte < 0x20 && !matches!(**byte, b'\t' | b'\n' | b'\r' | 0x0c))
        .count();
    suspicious * 100 < bytes.len()
}

fn map_io(path: &Path, error: &io::Error) -> Error {
    let code = match error.kind() {
        io::ErrorKind::NotFound => ErrorCode::NotFound,
        io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        _ => ErrorCode::Io,
    };
    Error::new(code, format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_win_over_anything_else() {
        assert_eq!(classify(b"\x89PNG\r\n\x1a\nrest"), ContentKind::Image);
        assert_eq!(classify(b"%PDF-1.7 and then text"), ContentKind::Pdf);
        assert_eq!(classify(b"PK\x03\x04zip"), ContentKind::Archive);
        assert_eq!(classify(b"\x7fELF\x02\x01"), ContentKind::Binary);
    }

    #[test]
    fn plain_text_is_text_and_a_nul_byte_is_not() {
        assert_eq!(classify(b"hello, world\nsecond line\n"), ContentKind::Text);
        assert_eq!(classify(b"hello\0world"), ContentKind::Binary);
    }

    #[test]
    fn a_lying_extension_cannot_make_a_binary_into_text() {
        // docs/VIEWER_PREVIEW.md 1: magic bytes override the extension. This
        // function never sees the name, which is the point.
        assert_eq!(classify(b"\x7fELF\x02\x01\x01\x00"), ContentKind::Binary);
    }

    #[test]
    fn utf16_with_a_bom_is_text_despite_its_nul_bytes() {
        let utf16: Vec<u8> = vec![0xff, 0xfe, b'h', 0, b'i', 0];
        assert_eq!(classify(&utf16), ContentKind::Text);
    }

    #[test]
    fn cjk_utf8_is_text() {
        assert_eq!(
            classify("中文檔案內容\n第二行\n".as_bytes()),
            ContentKind::Text
        );
    }

    #[test]
    fn an_empty_file_says_so_rather_than_guessing() {
        assert_eq!(classify(b""), ContentKind::Empty);
        assert!(ContentKind::Empty.is_textual());
    }

    #[test]
    fn every_kind_has_a_distinct_label_key() {
        let mut keys: Vec<_> = ContentKind::ALL.iter().map(|k| k.label_key()).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn classification_never_panics_on_arbitrary_bytes() {
        // Stands in for the fuzz target until fuzzing is wired up.
        for length in [0usize, 1, 2, 7, 64, 1024] {
            for seed in 0u8..=255 {
                let bytes: Vec<u8> = (0..length)
                    .map(|i| {
                        seed.wrapping_mul(31)
                            .wrapping_add(u8::try_from(i % 256).unwrap())
                    })
                    .collect();
                let _ = classify(&bytes);
            }
        }
    }
}
