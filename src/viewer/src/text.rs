//! A bounded window onto a text file.
//!
//! The file is never read whole. Opening builds a line index by scanning for
//! line breaks — one pass, no decoding, no allocation per line — and reading a
//! window decodes only the bytes those lines occupy. A 10 GB log therefore
//! opens in the time it takes to scan it once and costs one screen of memory
//! to display (`docs/VIEWER_PREVIEW.md` §4.1).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use jtf_core::{Error, ErrorCode};
use jtf_jobs::CancellationToken;

/// How much is read at a time while indexing.
const INDEX_CHUNK: usize = 1 << 20;

/// The largest single line that will be decoded for display.
///
/// A file with no line breaks at all is one enormous line; without a cap the
/// viewer would try to lay out gigabytes of text in one row
/// (`docs/UI_TEST_PLAN.md` VIEW-004).
const MAX_LINE_BYTES: u64 = 1 << 20;

/// Text encodings the viewer offers.
///
/// `Auto` inspects the content; the rest are explicit overrides, because
/// detection is a guess and the user is sometimes right when it is wrong
/// (`docs/VIEWER_PREVIEW.md` §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Encoding {
    /// Detect from the byte-order mark, then from the content.
    #[default]
    Auto,
    /// UTF-8.
    Utf8,
    /// UTF-16, little endian.
    Utf16Le,
    /// UTF-16, big endian.
    Utf16Be,
    /// Traditional Chinese, the encoding this project's users have the most
    /// legacy files in.
    Big5,
    /// Simplified Chinese.
    Gb18030,
    /// Japanese.
    ShiftJis,
    /// Korean.
    EucKr,
    /// Western European, and the fallback that can decode any byte sequence.
    Latin1,
}

impl Encoding {
    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Auto => "encoding.auto",
            Self::Utf8 => "encoding.utf8",
            Self::Utf16Le => "encoding.utf16le",
            Self::Utf16Be => "encoding.utf16be",
            Self::Big5 => "encoding.big5",
            Self::Gb18030 => "encoding.gb18030",
            Self::ShiftJis => "encoding.shift_jis",
            Self::EucKr => "encoding.euc_kr",
            Self::Latin1 => "encoding.latin1",
        }
    }

    /// Every encoding, in the order the UI should list them.
    pub const ALL: &'static [Self] = &[
        Self::Auto,
        Self::Utf8,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::Big5,
        Self::Gb18030,
        Self::ShiftJis,
        Self::EucKr,
        Self::Latin1,
    ];

    const fn as_encoding_rs(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            Self::Auto => None,
            Self::Utf8 => Some(encoding_rs::UTF_8),
            Self::Utf16Le => Some(encoding_rs::UTF_16LE),
            Self::Utf16Be => Some(encoding_rs::UTF_16BE),
            Self::Big5 => Some(encoding_rs::BIG5),
            Self::Gb18030 => Some(encoding_rs::GB18030),
            Self::ShiftJis => Some(encoding_rs::SHIFT_JIS),
            Self::EucKr => Some(encoding_rs::EUC_KR),
            Self::Latin1 => Some(encoding_rs::WINDOWS_1252),
        }
    }
}

/// What line breaks a file uses.
///
/// Shown rather than normalized: a file with mixed endings is telling you
/// something, and silently hiding it loses that (`docs/VIEWER_PREVIEW.md`
/// §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    /// Unix.
    #[default]
    Lf,
    /// Windows.
    Crlf,
    /// Classic Mac.
    Cr,
    /// More than one style in the same file.
    Mixed,
    /// No line break at all.
    None,
}

impl LineEnding {
    /// Localization key for the label.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Lf => "line_ending.lf",
            Self::Crlf => "line_ending.crlf",
            Self::Cr => "line_ending.cr",
            Self::Mixed => "line_ending.mixed",
            Self::None => "line_ending.none",
        }
    }

    /// Every value, for exhaustive tests and catalogue parity.
    pub const ALL: &'static [Self] = &[Self::Lf, Self::Crlf, Self::Cr, Self::Mixed, Self::None];

    fn from_counts(lf: u64, crlf: u64, cr: u64) -> Self {
        let styles = u8::from(lf > 0) + u8::from(crlf > 0) + u8::from(cr > 0);
        match styles {
            0 => Self::None,
            1 if crlf > 0 => Self::Crlf,
            1 if cr > 0 => Self::Cr,
            1 => Self::Lf,
            _ => Self::Mixed,
        }
    }
}

/// One screenful of decoded lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextWindow {
    /// Index of the first line, zero-based.
    pub first_line: u64,
    /// The decoded lines, without their line breaks.
    pub lines: Vec<String>,
    /// Whether any line in this window was truncated at [`MAX_LINE_BYTES`].
    pub truncated: bool,
}

/// A text file, indexed but not loaded.
///
/// `Debug` deliberately omits the offset table: printing ten million line
/// offsets is not a diagnostic, it is a denial of service on your terminal.
pub struct TextView {
    file: File,
    /// Byte offset of the start of every line.
    ///
    /// Eight bytes per line: a 10 million line log costs 80 MB of index, which
    /// is the honest price of random access and is reported to the caller.
    offsets: Vec<u64>,
    size: u64,
    encoding: Encoding,
    detected: Encoding,
    line_ending: LineEnding,
}

#[allow(
    clippy::missing_fields_in_debug,
    reason = "printing ten million line offsets is not a diagnostic"
)]
impl core::fmt::Debug for TextView {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextView")
            .field("size", &self.size)
            .field("lines", &self.offsets.len())
            .field("encoding", &self.effective_encoding())
            .field("line_ending", &self.line_ending)
            .finish()
    }
}

impl TextView {
    /// Open and index a file.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports, or [`ErrorCode::Cancelled`] if the
    /// indexing pass was interrupted.
    pub fn open(path: &Path, cancel: &CancellationToken) -> Result<Self, Error> {
        let mut file = File::open(path).map_err(|e| map_io(path, &e))?;
        let size = file.metadata().map_err(|e| map_io(path, &e))?.len();

        let mut offsets = vec![0u64];
        let mut buffer = vec![0u8; INDEX_CHUNK];
        let mut position = 0u64;
        let mut lf = 0u64;
        let mut crlf = 0u64;
        let mut cr = 0u64;
        let mut previous_was_cr = false;
        let mut head = Vec::new();

        loop {
            if cancel.is_cancelled() {
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            let read = file.read(&mut buffer).map_err(|e| map_io(path, &e))?;
            if read == 0 {
                break;
            }
            if head.len() < 4096 {
                head.extend_from_slice(&buffer[..read.min(4096)]);
            }
            for (index, byte) in buffer[..read].iter().enumerate() {
                let at = position + index as u64;
                match byte {
                    b'\n' => {
                        if previous_was_cr {
                            crlf += 1;
                        } else {
                            lf += 1;
                        }
                        previous_was_cr = false;
                        offsets.push(at + 1);
                    }
                    b'\r' => {
                        if previous_was_cr {
                            cr += 1;
                            offsets.push(at);
                        }
                        previous_was_cr = true;
                    }
                    _ => {
                        if previous_was_cr {
                            cr += 1;
                            offsets.push(at);
                        }
                        previous_was_cr = false;
                    }
                }
            }
            position += read as u64;
        }
        if previous_was_cr {
            cr += 1;
        }
        // A trailing newline produces an offset at end-of-file; that is not a
        // line, it is the absence of one.
        if offsets.last().is_some_and(|last| *last >= size) && offsets.len() > 1 {
            offsets.pop();
        }
        // An empty file has no lines, not one empty line.
        if size == 0 {
            offsets.clear();
        }

        let detected = detect_encoding(&head);
        Ok(Self {
            file,
            offsets,
            size,
            encoding: Encoding::Auto,
            detected,
            line_ending: LineEnding::from_counts(lf, crlf, cr),
        })
    }

    /// How many lines the file has.
    pub fn line_count(&self) -> u64 {
        self.offsets.len() as u64
    }

    /// The file's size in bytes.
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// Bytes the line index occupies, so the UI can be honest about the cost.
    pub fn index_bytes(&self) -> usize {
        self.offsets.len() * std::mem::size_of::<u64>()
    }

    /// What line endings the file uses.
    pub const fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// The encoding detection chose.
    pub const fn detected_encoding(&self) -> Encoding {
        self.detected
    }

    /// The encoding in use, resolving `Auto` to what was detected.
    pub const fn effective_encoding(&self) -> Encoding {
        match self.encoding {
            Encoding::Auto => self.detected,
            explicit => explicit,
        }
    }

    /// Override the encoding. `Auto` returns to detection.
    pub fn set_encoding(&mut self, encoding: Encoding) {
        self.encoding = encoding;
    }

    /// Decode `count` lines starting at `first`.
    ///
    /// # Errors
    ///
    /// Whatever the filesystem reports.
    pub fn window(&mut self, first: u64, count: usize) -> Result<TextWindow, Error> {
        let total = self.line_count();
        if first >= total || count == 0 {
            return Ok(TextWindow {
                first_line: first,
                lines: Vec::new(),
                truncated: false,
            });
        }
        let last = (first + count as u64).min(total);
        // Checked conversions: on a 32-bit target a file can have more lines
        // than an index can address, and truncating would read the wrong part
        // of the file rather than failing.
        let Ok(first_index) = usize::try_from(first) else {
            return Ok(TextWindow {
                first_line: first,
                lines: Vec::new(),
                truncated: false,
            });
        };
        let start = self.offsets[first_index];
        let end = usize::try_from(last)
            .ok()
            .filter(|_| last < total)
            .map_or(self.size, |index| self.offsets[index]);

        let span = end.saturating_sub(start);
        let capped = span.min(MAX_LINE_BYTES * count as u64);
        let truncated = capped < span;

        self.file
            .seek(SeekFrom::Start(start))
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;
        let mut bytes = vec![0u8; usize::try_from(capped).unwrap_or(0)];
        self.file
            .read_exact(&mut bytes)
            .map_err(|e| Error::new(ErrorCode::Io, e.to_string()))?;

        let text = decode(&bytes, self.effective_encoding());
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
            .collect();
        // Splitting on the final newline leaves an empty tail that is not a
        // line of the file.
        if lines.last().is_some_and(String::is_empty) && lines.len() > 1 {
            lines.pop();
        }
        lines.truncate(count);

        Ok(TextWindow {
            first_line: first,
            lines,
            truncated,
        })
    }
}

/// Decode with an explicit encoding, replacing anything malformed.
///
/// Never fails: a viewer that refuses to show a file because one byte is wrong
/// is less useful than one that shows the byte as U+FFFD.
fn decode(bytes: &[u8], encoding: Encoding) -> String {
    let coder = encoding.as_encoding_rs().unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = coder.decode(bytes);
    text.into_owned()
}

/// Guess an encoding from a prefix.
///
/// A byte-order mark is definitive. Otherwise valid UTF-8 is overwhelmingly
/// the right answer, and anything else falls back to Latin-1, which can decode
/// any byte sequence and so never leaves the user with nothing.
fn detect_encoding(head: &[u8]) -> Encoding {
    if head.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Encoding::Utf8;
    }
    if head.starts_with(&[0xff, 0xfe]) {
        return Encoding::Utf16Le;
    }
    if head.starts_with(&[0xfe, 0xff]) {
        return Encoding::Utf16Be;
    }
    if std::str::from_utf8(head).is_ok() {
        return Encoding::Utf8;
    }
    // A truncated multi-byte character at the end of the sample is not a
    // reason to reject UTF-8 for the whole file.
    for trim in 1..=3usize.min(head.len()) {
        if std::str::from_utf8(&head[..head.len() - trim]).is_ok() {
            return Encoding::Utf8;
        }
    }
    Encoding::Latin1
}

fn map_io(path: &Path, error: &io::Error) -> Error {
    let code = match error.kind() {
        io::ErrorKind::NotFound => ErrorCode::NotFound,
        io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        _ => ErrorCode::Io,
    };
    Error::new(code, format!("{}: {error}", path.display()))
}
