//! What to look for, and finding it.
//!
//! Four ways of saying the same kind of thing — a run of bytes — because the
//! thing someone is looking for is known to them in different terms depending
//! on why they are looking. A file signature is bytes (`50 4B 03 04`); a name
//! inside a record is text; a field is a number; and a pattern with holes in
//! it is how you find a run of code whose middle changes.

use jtf_core::{Error, ErrorCode};

/// How the text in the search box should be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `50 4B 03 04`, and `??` for a byte that may be anything.
    Hex,
    /// Bytes of the text as typed.
    Utf8,
    /// UTF-16, little end first.
    Utf16Le,
    /// UTF-16, big end first.
    Utf16Be,
    /// Latin-1, which is what "ASCII" usually means in this context and is
    /// one byte per character for everything it can represent.
    Latin1,
    /// A number, written out in the given width and byte order.
    Integer {
        /// How many bytes the number occupies.
        width: Width,
        /// Whether the low byte comes first.
        little_endian: bool,
    },
}

/// How wide an integer needle is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// One byte.
    U8,
    /// Two bytes.
    U16,
    /// Four bytes.
    U32,
    /// Eight bytes.
    U64,
}

impl Width {
    const fn bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
            Self::U64 => 8,
        }
    }
}

/// A compiled needle: one entry per byte, `None` meaning "anything".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle(Vec<Option<u8>>);

impl Needle {
    /// How many bytes a match is.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is nothing to look for.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The bytes, where every one is known. `None` if it has wildcards —
    /// which is what "replace with" has to refuse, since there is nothing to
    /// put in the holes.
    pub fn literal(&self) -> Option<Vec<u8>> {
        self.0.iter().copied().collect()
    }

    /// Whether `window` matches, position for position.
    fn matches(&self, window: &[u8]) -> bool {
        window.len() >= self.0.len()
            && self
                .0
                .iter()
                .zip(window)
                .all(|(want, got)| want.is_none_or(|w| w == *got))
    }

    /// Compile the search box's text.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::InvalidPath`] when the text cannot be read as the chosen
    /// kind — an odd hex digit, a number too big for its width. Refused
    /// rather than trimmed to something searchable, because a search that
    /// quietly looked for something else is worse than one that did not run.
    pub fn compile(text: &str, kind: Kind) -> Result<Self, Error> {
        let bytes = match kind {
            Kind::Hex => return Self::compile_hex(text),
            Kind::Utf8 => text.as_bytes().to_vec(),
            Kind::Latin1 => text
                .chars()
                .map(|c| {
                    u8::try_from(u32::from(c)).map_err(|_| {
                        Error::new(
                            ErrorCode::InvalidPath,
                            format!("{c:?} cannot be written in Latin-1"),
                        )
                    })
                })
                .collect::<Result<Vec<u8>, Error>>()?,
            Kind::Utf16Le => text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
            Kind::Utf16Be => text
                .encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect(),
            Kind::Integer {
                width,
                little_endian,
            } => Self::integer(text, width, little_endian)?,
        };
        Ok(Self(bytes.into_iter().map(Some).collect()))
    }

    fn compile_hex(text: &str) -> Result<Self, Error> {
        let mut out = Vec::new();
        // Split on anything that is not a hex digit or a question mark, so
        // `50 4B`, `50,4B`, `504B` and `\x50\x4B` all work without the user
        // being told which one this program wanted.
        let cleaned: String = text
            .chars()
            .filter(|c| c.is_ascii_hexdigit() || *c == '?')
            .collect();
        if cleaned.is_empty() {
            return Err(Error::new(
                ErrorCode::InvalidPath,
                format!("{text}: no hex digits in there"),
            ));
        }
        if !cleaned.len().is_multiple_of(2) {
            return Err(Error::new(
                ErrorCode::InvalidPath,
                format!("{text}: an odd number of hex digits - a byte is two"),
            ));
        }
        let chars: Vec<char> = cleaned.chars().collect();
        for pair in chars.chunks(2) {
            if pair[0] == '?' && pair[1] == '?' {
                out.push(None);
                continue;
            }
            if pair[0] == '?' || pair[1] == '?' {
                return Err(Error::new(
                    ErrorCode::InvalidPath,
                    format!("{text}: half a wildcard byte - write ?? for a whole one"),
                ));
            }
            let high = u8::try_from(pair[0].to_digit(16).unwrap_or(0)).unwrap_or(0);
            let low = u8::try_from(pair[1].to_digit(16).unwrap_or(0)).unwrap_or(0);
            out.push(Some((high << 4) | low));
        }
        Ok(Self(out))
    }

    fn integer(text: &str, width: Width, little_endian: bool) -> Result<Vec<u8>, Error> {
        let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let value: u64 = if let Some(rest) = cleaned.strip_prefix("0x") {
            u64::from_str_radix(rest, 16)
        } else {
            cleaned.parse()
        }
        .map_err(|_| {
            Error::new(
                ErrorCode::InvalidPath,
                format!("{text}: not a whole number"),
            )
        })?;

        let n = width.bytes();
        if n < 8 && value >= (1u64 << (n * 8)) {
            return Err(Error::new(
                ErrorCode::InvalidPath,
                format!("{value} does not fit in {n} byte(s)"),
            ));
        }
        let full = value.to_le_bytes();
        let mut bytes = full[..n].to_vec();
        if !little_endian {
            bytes.reverse();
        }
        Ok(bytes)
    }
}

/// Somewhere to read bytes from, so finding does not need the whole file.
pub trait Bytes {
    /// Total length.
    fn len(&self) -> u64;
    /// Whether there is nothing.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Read up to `len` bytes at `offset`, short at the end.
    ///
    /// # Errors
    ///
    /// Whatever the underlying store reports.
    fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, Error>;
}

/// How much is read at a time while scanning.
const CHUNK: usize = 1 << 16;

/// The first match at or after `from`, or `None`.
///
/// # Errors
///
/// Whatever reading reports.
pub fn find_forward(
    source: &mut impl Bytes,
    needle: &Needle,
    from: u64,
) -> Result<Option<u64>, Error> {
    if needle.is_empty() || source.len() < needle.len() as u64 {
        return Ok(None);
    }
    let last_start = source.len() - needle.len() as u64;
    let mut at = from.min(last_start.saturating_add(1));
    while at <= last_start {
        // Overlapping windows, or a match lying across a chunk boundary
        // would be missed - the classic way a chunked search is wrong.
        let want = CHUNK + needle.len() - 1;
        let window = source.read_at(at, want)?;
        if window.len() < needle.len() {
            break;
        }
        for i in 0..=(window.len() - needle.len()) {
            if needle.matches(&window[i..]) {
                return Ok(Some(at + i as u64));
            }
        }
        at += CHUNK as u64;
    }
    Ok(None)
}

/// The last match starting strictly before `before`, or `None`.
///
/// # Errors
///
/// Whatever reading reports.
pub fn find_backward(
    source: &mut impl Bytes,
    needle: &Needle,
    before: u64,
) -> Result<Option<u64>, Error> {
    if needle.is_empty() || source.len() < needle.len() as u64 || before == 0 {
        return Ok(None);
    }
    let mut end = before.min(source.len());
    loop {
        let start = end.saturating_sub(CHUNK as u64);
        let want = usize::try_from(end - start).unwrap_or(0) + needle.len() - 1;
        let window = source.read_at(start, want)?;
        if window.len() >= needle.len() {
            for i in (0..=(window.len() - needle.len())).rev() {
                let at = start + i as u64;
                if at < before && needle.matches(&window[i..]) {
                    return Ok(Some(at));
                }
            }
        }
        if start == 0 {
            return Ok(None);
        }
        end = start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Slice(Vec<u8>);

    impl Bytes for Slice {
        fn len(&self) -> u64 {
            self.0.len() as u64
        }
        fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, Error> {
            let from = usize::try_from(offset).unwrap_or(usize::MAX).min(self.0.len());
            let to = from.saturating_add(len).min(self.0.len());
            Ok(self.0[from..to].to_vec())
        }
    }

    fn hex(text: &str) -> Needle {
        Needle::compile(text, Kind::Hex).unwrap()
    }

    #[test]
    fn hex_is_read_however_it_is_spaced() {
        let want = hex("50 4B 03 04");
        assert_eq!(want.literal(), Some(vec![0x50, 0x4b, 0x03, 0x04]));
        assert_eq!(hex("504B0304"), want);
        assert_eq!(hex("50,4b,03,04"), want);
        assert_eq!(hex("\\x50\\x4B\\x03\\x04"), want);
    }

    #[test]
    fn a_wildcard_byte_matches_anything_but_still_takes_a_place() {
        let want = hex("48 8B ?? ?? 89");
        assert_eq!(want.len(), 5);
        assert_eq!(want.literal(), None, "a hole is not a byte to write");

        let mut data = Slice(vec![0x00, 0x48, 0x8b, 0xff, 0x01, 0x89, 0x00]);
        assert_eq!(find_forward(&mut data, &want, 0).unwrap(), Some(1));
    }

    #[test]
    fn bad_hex_is_refused_rather_than_searched_for_as_something_else() {
        for text in ["50 4", "5", "zz", "", "4? 8B"] {
            assert!(
                Needle::compile(text, Kind::Hex).is_err(),
                "{text:?} compiled into a needle"
            );
        }
    }

    #[test]
    fn text_compiles_in_each_encoding() {
        assert_eq!(
            Needle::compile("Hi", Kind::Utf8).unwrap().literal(),
            Some(vec![b'H', b'i'])
        );
        assert_eq!(
            Needle::compile("Hi", Kind::Utf16Le).unwrap().literal(),
            Some(vec![b'H', 0, b'i', 0])
        );
        assert_eq!(
            Needle::compile("Hi", Kind::Utf16Be).unwrap().literal(),
            Some(vec![0, b'H', 0, b'i'])
        );
        assert_eq!(
            Needle::compile("é", Kind::Latin1).unwrap().literal(),
            Some(vec![0xe9])
        );
        assert!(
            Needle::compile("中", Kind::Latin1).is_err(),
            "a character Latin-1 cannot hold was silently mangled"
        );
    }

    #[test]
    fn integers_are_laid_out_in_the_width_and_order_asked_for() {
        let le = Needle::compile(
            "1000",
            Kind::Integer {
                width: Width::U32,
                little_endian: true,
            },
        )
        .unwrap();
        assert_eq!(le.literal(), Some(vec![0xe8, 0x03, 0x00, 0x00]));

        let be = Needle::compile(
            "0x3e8",
            Kind::Integer {
                width: Width::U32,
                little_endian: false,
            },
        )
        .unwrap();
        assert_eq!(be.literal(), Some(vec![0x00, 0x00, 0x03, 0xe8]));

        assert!(
            Needle::compile(
                "70000",
                Kind::Integer {
                    width: Width::U16,
                    little_endian: true
                }
            )
            .is_err(),
            "a value too big for its width was truncated instead of refused"
        );
    }

    #[test]
    fn forward_and_backward_find_every_occurrence_in_order() {
        let mut data = Slice(b"abXYabXYab".to_vec());
        let want = Needle::compile("ab", Kind::Utf8).unwrap();

        assert_eq!(find_forward(&mut data, &want, 0).unwrap(), Some(0));
        assert_eq!(find_forward(&mut data, &want, 1).unwrap(), Some(4));
        assert_eq!(find_forward(&mut data, &want, 5).unwrap(), Some(8));
        assert_eq!(find_forward(&mut data, &want, 9).unwrap(), None);

        assert_eq!(find_backward(&mut data, &want, 10).unwrap(), Some(8));
        assert_eq!(find_backward(&mut data, &want, 8).unwrap(), Some(4));
        assert_eq!(find_backward(&mut data, &want, 4).unwrap(), Some(0));
        assert_eq!(find_backward(&mut data, &want, 0).unwrap(), None);
    }

    #[test]
    fn a_match_lying_across_a_chunk_boundary_is_still_found() {
        // The way a chunked search is usually wrong. The needle straddles the
        // seam between two reads.
        let mut bytes = vec![0u8; CHUNK * 2];
        let at = CHUNK - 2;
        bytes[at..at + 4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let mut data = Slice(bytes);
        let want = hex("DE AD BE EF");
        assert_eq!(find_forward(&mut data, &want, 0).unwrap(), Some(at as u64));
        assert_eq!(
            find_backward(&mut data, &want, CHUNK as u64 * 2).unwrap(),
            Some(at as u64)
        );
    }

    #[test]
    fn a_needle_longer_than_the_file_finds_nothing_rather_than_panicking() {
        let mut data = Slice(b"ab".to_vec());
        let want = Needle::compile("abcdef", Kind::Utf8).unwrap();
        assert_eq!(find_forward(&mut data, &want, 0).unwrap(), None);
        assert_eq!(find_backward(&mut data, &want, 2).unwrap(), None);
    }
}
