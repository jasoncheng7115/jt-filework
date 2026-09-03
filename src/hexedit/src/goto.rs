//! Working out which offset someone means.
//!
//! One box, several ways of writing a position, because a person reading a
//! file format thinks in all of them: `0x1F4` from a spec, `500` from a
//! calculator, `+0x200` for the next record, `-512` to step back a sector,
//! and `end-4` for a trailer.

use jtf_core::{Error, ErrorCode};

/// Where an expression is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Start,
    Cursor,
    End,
}

/// Parse a position and resolve it against where the cursor is now.
///
/// Accepted, case-insensitively and ignoring spaces:
///
/// - `1f4`, `0x1F4`, `$1F4` — hexadecimal, the default, because a hex window
///   shows offsets in hex and typing what is on screen must work.
/// - `500.` or `0n500` — decimal. A trailing dot is the shortest way to say
///   "this one is decimal" and `0n` is what debuggers use.
/// - `+0x200`, `-512.` — relative to the cursor.
/// - `end`, `end-4`, `end-0x10` — from the end of the file.
///
/// # Errors
///
/// [`ErrorCode::InvalidPath`] with the text that could not be read as a
/// position. Nothing is guessed: an expression that is not understood must
/// not quietly become offset zero.
pub fn resolve(text: &str, cursor: u64, len: u64) -> Result<u64, Error> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    let lowered = cleaned.to_ascii_lowercase();
    if lowered.is_empty() {
        return Err(bad(text));
    }

    let (anchor, rest) = if let Some(rest) = lowered.strip_prefix("end") {
        (Anchor::End, rest)
    } else if lowered.starts_with('+') || lowered.starts_with('-') {
        (Anchor::Cursor, lowered.as_str())
    } else {
        (Anchor::Start, lowered.as_str())
    };

    // An anchor on its own is a position.
    if rest.is_empty() {
        return Ok(match anchor {
            Anchor::Start => 0,
            Anchor::Cursor => cursor,
            Anchor::End => len,
        });
    }

    let (negative, digits) = match rest.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, rest.strip_prefix('+').unwrap_or(rest)),
    };
    if digits.is_empty() {
        return Err(bad(text));
    }
    let magnitude = number(digits).ok_or_else(|| bad(text))?;

    let base = match anchor {
        Anchor::Start => 0,
        Anchor::Cursor => cursor,
        Anchor::End => len,
    };
    let resolved = if negative {
        base.checked_sub(magnitude).ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidPath,
                format!("{text}: that is before the start of the file"),
            )
        })?
    } else {
        base.checked_add(magnitude).ok_or_else(|| bad(text))?
    };

    // Clamped to the end rather than refused: asking to go past the end of a
    // file means the end, and an error there would be pedantry.
    Ok(resolved.min(len))
}

/// Read one number, hexadecimal unless it says otherwise.
fn number(text: &str) -> Option<u64> {
    if let Some(rest) = text.strip_prefix("0x") {
        return u64::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = text.strip_prefix('$') {
        return u64::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = text.strip_prefix("0n") {
        return rest.parse().ok();
    }
    if let Some(rest) = text.strip_suffix('.') {
        return rest.parse().ok();
    }
    u64::from_str_radix(text, 16).ok()
}

fn bad(text: &str) -> Error {
    Error::new(
        ErrorCode::InvalidPath,
        format!("{text}: not a position in this file"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: u64 = 0x1000;
    const CURSOR: u64 = 0x800;

    fn at(text: &str) -> u64 {
        resolve(text, CURSOR, LEN).unwrap()
    }

    #[test]
    fn a_bare_number_is_hexadecimal_because_that_is_what_is_on_screen() {
        assert_eq!(at("1f4"), 0x1f4);
        assert_eq!(at("1F4"), 0x1f4);
        assert_eq!(at("0x1f4"), 0x1f4);
        assert_eq!(at("$1f4"), 0x1f4);
    }

    #[test]
    fn decimal_has_to_be_asked_for_and_there_are_two_ways() {
        assert_eq!(at("500."), 500);
        assert_eq!(at("0n500"), 500);
        assert_ne!(at("500"), 500, "a bare number silently changed meaning");
        assert_eq!(at("500"), 0x500);
    }

    #[test]
    fn a_sign_makes_it_relative_to_the_cursor() {
        assert_eq!(at("+0x200"), CURSOR + 0x200);
        assert_eq!(at("-512."), CURSOR - 512);
        assert_eq!(at("+10"), CURSOR + 0x10);
    }

    #[test]
    fn end_counts_back_from_the_end_of_the_file() {
        assert_eq!(at("end"), LEN);
        assert_eq!(at("end-4"), LEN - 4);
        assert_eq!(at("end-0x10"), LEN - 0x10);
        assert_eq!(at("end-16."), LEN - 16);
    }

    #[test]
    fn spaces_and_underscores_are_ignored_so_a_pasted_offset_works() {
        assert_eq!(at(" 0x 1f4 "), 0x1f4);
        assert_eq!(at("end - 4"), LEN - 4);
        assert_eq!(at("1_f4"), 0x1f4);
    }

    #[test]
    fn past_the_end_is_the_end_rather_than_an_error() {
        assert_eq!(at("0xffffff"), LEN);
        assert_eq!(at("+0xffffff"), LEN);
    }

    #[test]
    fn before_the_start_is_refused_rather_than_wrapped() {
        let err = resolve("-0x900", CURSOR, LEN).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidPath);
        assert!(err.context().contains("before the start"), "{err}");
    }

    #[test]
    fn nonsense_is_refused_rather_than_read_as_zero() {
        for text in ["", "  ", "zz", "0x", "end-", "+", "-", "12g4", "0n1f"] {
            assert!(
                resolve(text, CURSOR, LEN).is_err(),
                "{text:?} was accepted and would have moved the cursor somewhere"
            );
        }
    }
}
