//! A hex window.
//!
//! The universal fallback: every file can be shown as hex, so an unrecognised
//! type is never an error state (`docs/VIEWER_PREVIEW.md` §1).

use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use jtf_core::{Error, ErrorCode};

/// Bytes shown per row.
pub const ROW_BYTES: usize = 16;

/// One screenful of bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexWindow {
    /// Offset of the first byte.
    pub offset: u64,
    /// The bytes themselves.
    pub bytes: Vec<u8>,
}

impl HexWindow {
    /// Format as `offset  hex bytes  |ascii|` rows.
    ///
    /// Printable ASCII shows as itself and everything else as `.`; the byte
    /// values are already in the hex column, so the text column exists to make
    /// strings findable by eye, not to be another encoding.
    pub fn rows(&self) -> Vec<String> {
        self.bytes
            .chunks(ROW_BYTES)
            .enumerate()
            .map(|(index, chunk)| {
                let offset = self.offset + (index * ROW_BYTES) as u64;
                let mut hex = String::with_capacity(ROW_BYTES * 3);
                let mut text = String::with_capacity(ROW_BYTES);
                for (i, byte) in chunk.iter().enumerate() {
                    if i == ROW_BYTES / 2 {
                        hex.push(' ');
                    }
                    let _ = write!(hex, "{byte:02x} ");
                    text.push(if (0x20..0x7f).contains(byte) {
                        char::from(*byte)
                    } else {
                        '.'
                    });
                }
                format!("{offset:08x}  {hex:<49} |{text}|")
            })
            .collect()
    }
}

/// A file, read a window at a time.
pub struct HexView {
    file: File,
    size: u64,
}

impl HexView {
    /// Open a file for hex viewing.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let file = File::open(path)
            .map_err(|e| Error::new(ErrorCode::Io, format!("{}: {e}", path.display())))?;
        let size = file
            .metadata()
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?
            .len();
        Ok(Self { file, size })
    }

    /// The file's size.
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// How many rows the whole file would occupy.
    pub const fn row_count(&self) -> u64 {
        self.size.div_ceil(ROW_BYTES as u64)
    }

    /// Read `rows` rows starting at row `first`.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn window(&mut self, first_row: u64, rows: usize) -> Result<HexWindow, Error> {
        let offset = first_row * ROW_BYTES as u64;
        if offset >= self.size {
            return Ok(HexWindow {
                offset,
                bytes: Vec::new(),
            });
        }
        let wanted = (rows * ROW_BYTES) as u64;
        let length = wanted.min(self.size - offset);

        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        let mut bytes = vec![0u8; usize::try_from(length).unwrap_or(0)];
        self.file
            .read_exact(&mut bytes)
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        Ok(HexWindow { offset, bytes })
    }
}
