//! Getting bytes out to somewhere else, and back in again.
//!
//! Copying out of a hex window is usually copying *into* something: a header
//! into a C struct, a signature into a test, a key into a script. So the
//! formats are the ones those places want, already punctuated, rather than a
//! hex dump the person then has to reshape by hand.
//!
//! Coming back the other way there is no menu, because a paste arrives
//! already looking like one thing or another and asking which would be asking
//! about something already on the screen.

use std::fmt::Write as _;

use jtf_core::{Error, ErrorCode};

/// How bytes are written out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The bytes themselves, for pasting into another binary.
    Raw,
    /// `504b0304`.
    HexString,
    /// `50 4B 03 04`, which is how a signature is written in a spec.
    HexSpaced,
    /// `{ 0x50, 0x4b, 0x03, 0x04 }`.
    CArray,
    /// `[0x50, 0x4b, 0x03, 0x04]`.
    RustArray,
    /// `b"\x50\x4b\x03\x04"`.
    PythonBytes,
    /// Base64, standard alphabet with padding.
    Base64,
}

impl Format {
    /// Every format, in the order the menu offers them.
    pub const ALL: &'static [Self] = &[
        Self::Raw,
        Self::HexString,
        Self::HexSpaced,
        Self::CArray,
        Self::RustArray,
        Self::PythonBytes,
        Self::Base64,
    ];

    /// Catalogue key for the menu entry.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Raw => "hex.copy.raw",
            Self::HexString => "hex.copy.hex_string",
            Self::HexSpaced => "hex.copy.hex_spaced",
            Self::CArray => "hex.copy.c_array",
            Self::RustArray => "hex.copy.rust_array",
            Self::PythonBytes => "hex.copy.python_bytes",
            Self::Base64 => "hex.copy.base64",
        }
    }
}

/// Render `bytes` in `format`.
///
/// `Raw` comes back as the bytes reinterpreted as Latin-1, which is the only
/// lossless way to put arbitrary bytes into a string; the caller putting them
/// on the clipboard as text is what makes that the right shape.
pub fn render(bytes: &[u8], format: Format) -> String {
    match format {
        Format::Raw => bytes.iter().map(|b| char::from(*b)).collect(),
        Format::HexString => bytes.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        }),
        Format::HexSpaced => bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" "),
        Format::CArray => wrap("{ ", bytes, " }", 12),
        Format::RustArray => wrap("[", bytes, "]", 12),
        Format::PythonBytes => {
            let inner = bytes.iter().fold(String::new(), |mut s, b| {
                let _ = write!(s, "\\x{b:02x}");
                s
            });
            format!("b\"{inner}\"")
        }
        Format::Base64 => base64(bytes),
    }
}

/// `0x..` elements, wrapped so a long selection is still readable.
fn wrap(open: &str, bytes: &[u8], close: &str, per_line: usize) -> String {
    let mut out = String::from(open);
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(',');
            if i.is_multiple_of(per_line) {
                out.push('\n');
                out.push_str(&" ".repeat(open.len()));
            } else {
                out.push(' ');
            }
        }
        let _ = write!(out, "0x{byte:02x}");
    }
    out.push_str(close);
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (usize::from(b[0]) << 16) | (usize::from(b[1]) << 8) | usize::from(b[2]);
        out.push(char::from(B64[(n >> 18) & 63]));
        out.push(char::from(B64[(n >> 12) & 63]));
        out.push(if chunk.len() > 1 {
            char::from(B64[(n >> 6) & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(B64[n & 63])
        } else {
            '='
        });
    }
    out
}

fn un_base64(text: &str) -> Option<Vec<u8>> {
    let cleaned: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks(4) {
        let mut n = 0usize;
        let mut pad = 0;
        for (i, c) in chunk.iter().enumerate() {
            if *c == b'=' {
                // Padding is only ever the last one or two.
                if i < 2 {
                    return None;
                }
                pad += 1;
                n <<= 6;
                continue;
            }
            if pad > 0 {
                return None;
            }
            let value = B64.iter().position(|x| x == c)?;
            n = (n << 6) | value;
        }
        out.push(u8::try_from((n >> 16) & 0xff).unwrap_or(0));
        if pad < 2 {
            out.push(u8::try_from((n >> 8) & 0xff).unwrap_or(0));
        }
        if pad < 1 {
            out.push(u8::try_from(n & 0xff).unwrap_or(0));
        }
    }
    Some(out)
}

/// What a pasted string turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pasted {
    /// What the pasted text turned out to be, as bytes.
    pub bytes: Vec<u8>,
    /// Catalogue key naming how it was read, so the window can say so rather
    /// than silently choosing.
    pub read_as: &'static str,
}

/// Work out what a pasted string is and turn it into bytes.
///
/// Tried in order of how unambiguous each one is. Hex first: a string of hex
/// digit pairs is almost never meant as its own text, and it is the format a
/// hex window is most often pasted into. Base64 next, but only when it has
/// the shape of base64 rather than merely the alphabet — `deadbeef` is both,
/// and it means hex here. Text last, which always succeeds, so a paste is
/// never refused.
///
/// # Errors
///
/// [`ErrorCode::InvalidPath`] only when there is nothing there at all.
pub fn parse_paste(text: &str) -> Result<Pasted, Error> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(Error::new(ErrorCode::InvalidPath, "nothing to paste"));
    }

    if let Some(bytes) = as_hex(trimmed) {
        return Ok(Pasted {
            bytes,
            read_as: "hex.paste.hex",
        });
    }
    if looks_like_base64(trimmed) {
        if let Some(bytes) = un_base64(trimmed) {
            return Ok(Pasted {
                bytes,
                read_as: "hex.paste.base64",
            });
        }
    }
    Ok(Pasted {
        bytes: trimmed.as_bytes().to_vec(),
        read_as: "hex.paste.text",
    })
}

/// Hex if, once the notation is peeled off, what is left is paired hex digits.
///
/// The wrappers are removed structurally rather than by dropping characters:
/// filtering out every `b` as the Python prefix also removes it as a hex
/// digit, which turned `504b0304` into seven digits and then into text.
fn as_hex(text: &str) -> Option<Vec<u8>> {
    let mut body = text.trim();

    // Python's bytes literal, only at the ends where it can occur.
    for (open, close) in [("b\"", '"'), ("B\"", '"'), ("b'", '\''), ("B'", '\'')] {
        if let Some(rest) = body.strip_prefix(open) {
            body = rest.strip_suffix(close)?;
            break;
        }
    }
    let body = body.trim();

    // A C or a Rust array.
    let body = body
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .or_else(|| body.strip_prefix('[').and_then(|r| r.strip_suffix(']')))
        .unwrap_or(body);

    // The per-byte prefixes become separators, so what is left is digits.
    let spaced = body.replace("0x", " ").replace("0X", " ").replace("\\x", " ");
    let digits: String = spaced
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .collect();

    if digits.is_empty() || !digits.len().is_multiple_of(2) {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let chars: Vec<char> = digits.chars().collect();
    Some(
        chars
            .chunks(2)
            .map(|p| {
                let high = u8::try_from(p[0].to_digit(16).unwrap_or(0)).unwrap_or(0);
                let low = u8::try_from(p[1].to_digit(16).unwrap_or(0)).unwrap_or(0);
                (high << 4) | low
            })
            .collect(),
    )
}

/// The shape of base64, not merely its alphabet.
fn looks_like_base64(text: &str) -> bool {
    let cleaned: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.len() < 4 || !cleaned.len().is_multiple_of(4) {
        return false;
    }
    let body_ok = cleaned
        .iter()
        .all(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=');
    // Something that is entirely hex digits is hex, whatever else it could
    // also be read as.
    let all_hex = cleaned.iter().all(char::is_ascii_hexdigit);
    body_ok && !all_hex
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: &[u8] = &[0x50, 0x4b, 0x03, 0x04];

    #[test]
    fn each_format_is_the_shape_the_place_it_is_going_wants() {
        assert_eq!(render(BYTES, Format::HexString), "504b0304");
        assert_eq!(render(BYTES, Format::HexSpaced), "50 4B 03 04");
        assert_eq!(render(BYTES, Format::CArray), "{ 0x50, 0x4b, 0x03, 0x04 }");
        assert_eq!(render(BYTES, Format::RustArray), "[0x50, 0x4b, 0x03, 0x04]");
        assert_eq!(render(BYTES, Format::PythonBytes), r#"b"\x50\x4b\x03\x04""#);
        assert_eq!(render(BYTES, Format::Base64), "UEsDBA==");
        assert_eq!(render(BYTES, Format::Raw).len(), 4);
    }

    #[test]
    fn base64_matches_what_every_other_tool_prints() {
        assert_eq!(render(b"", Format::Base64), "");
        assert_eq!(render(b"f", Format::Base64), "Zg==");
        assert_eq!(render(b"fo", Format::Base64), "Zm8=");
        assert_eq!(render(b"foo", Format::Base64), "Zm9v");
        assert_eq!(render(b"foob", Format::Base64), "Zm9vYg==");
        assert_eq!(render(b"fooba", Format::Base64), "Zm9vYmE=");
        assert_eq!(render(b"foobar", Format::Base64), "Zm9vYmFy");
    }

    #[test]
    fn everything_this_module_writes_pastes_back_as_the_bytes_it_came_from() {
        for format in Format::ALL {
            if *format == Format::Raw {
                continue; // Raw is bytes, not a notation to read back.
            }
            let text = render(BYTES, *format);
            let back = parse_paste(&text).unwrap();
            assert_eq!(back.bytes, BYTES, "{format:?} did not survive: {text}");
        }
    }

    #[test]
    fn a_long_array_wraps_and_still_reads_back() {
        let bytes: Vec<u8> = (0..40u8).collect();
        let text = render(&bytes, Format::RustArray);
        assert!(text.contains('\n'), "40 bytes on one line is not readable");
        assert_eq!(parse_paste(&text).unwrap().bytes, bytes);
    }

    #[test]
    fn hex_wins_over_base64_when_a_string_could_be_either() {
        // `deadbeef` is a valid base64 string and a valid hex one. In a hex
        // window it means hex.
        let got = parse_paste("deadbeef").unwrap();
        assert_eq!(got.bytes, vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(got.read_as, "hex.paste.hex");
    }

    #[test]
    fn real_base64_is_recognised() {
        let got = parse_paste("Zm9vYmFy").unwrap();
        assert_eq!(got.bytes, b"foobar");
        assert_eq!(got.read_as, "hex.paste.base64");
    }

    #[test]
    fn anything_else_is_taken_as_text_rather_than_refused() {
        let got = parse_paste("hello world").unwrap();
        assert_eq!(got.bytes, b"hello world");
        assert_eq!(got.read_as, "hex.paste.text");

        // An odd number of hex digits is not a run of bytes, so it is text.
        assert_eq!(parse_paste("abc").unwrap().read_as, "hex.paste.text");
    }

    #[test]
    fn an_empty_paste_is_the_one_thing_refused() {
        assert!(parse_paste("").is_err());
        assert!(parse_paste("   \n ").is_err());
    }

    #[test]
    fn every_format_has_a_label_to_show_and_they_are_all_different() {
        let keys: Vec<&str> = Format::ALL.iter().map(|f| f.label_key()).collect();
        let mut unique = keys.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(keys.len(), unique.len(), "two formats share a menu entry");
    }
}
