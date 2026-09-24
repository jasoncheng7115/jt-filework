//! What to call a copy kept beside the original.
//!
//! One rule for every place that needs it - a duplicate, a paste into the
//! folder the files came from, "keep both" when a copy or move meets a name
//! already taken, locally or on a server, and a collision in the trash - so
//! the same situation never produces two different names.
//!
//! `report.txt` becomes `report (1).txt`, then `report (2).txt`: the form
//! browsers and Windows use, and the one the project owner asked for.
//! (Finder's `report 2.txt` was used until 0.6.48.) A name that already ends
//! in a number in brackets carries on counting, so duplicating
//! `report (1).txt` gives `report (2).txt` rather than `report (1) (1).txt`.

use std::ffi::{OsStr, OsString};
use std::path::Path;

/// How many numbered names are tried before giving up. A folder that holds
/// all of them is a failure to report, not a loop to spin in.
pub const MAX_COPY_NUMBER: u32 = 999;

/// The names to try, in order, for a copy of `name` kept beside it.
///
/// The extension is what follows the last dot, as the platform's own path
/// handling reads it: `.bashrc` has none, and `archive.tar.gz` has `gz`, so
/// its copy is `archive.tar (1).gz`.
pub fn copy_names(name: &OsStr) -> impl Iterator<Item = OsString> {
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or(name).to_os_string();
    let extension = path.extension().map(OsStr::to_os_string);
    let (base, first) = strip_copy_number(&stem);

    (first..=MAX_COPY_NUMBER).map(move |n| {
        let mut candidate = base.clone();
        candidate.push(format!(" ({n})"));
        if let Some(extension) = &extension {
            candidate.push(".");
            candidate.push(extension);
        }
        candidate
    })
}

/// A stem without a trailing ` (n)`, and the number to start counting from.
///
/// Only for a stem that is valid UTF-8; anything else keeps its whole stem
/// and starts at 1, which is still a correct name, just not the tidiest.
fn strip_copy_number(stem: &OsStr) -> (OsString, u32) {
    let Some(text) = stem.to_str() else {
        return (stem.to_os_string(), 1);
    };
    if let Some(inner) = text.strip_suffix(')') {
        if let Some((base, digits)) = inner.rsplit_once(" (") {
            let is_number = !digits.is_empty()
                && digits.len() <= 3
                && digits.bytes().all(|b| b.is_ascii_digit());
            if is_number && !base.is_empty() {
                // Past the last number there is nothing to carry on to, so
                // the whole name is kept and counting starts again inside it.
                if let Ok(n) = digits.parse::<u32>() {
                    if n < MAX_COPY_NUMBER {
                        return (OsString::from(base), n + 1);
                    }
                }
            }
        }
    }
    (stem.to_os_string(), 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(name: &str, count: usize) -> Vec<String> {
        copy_names(OsStr::new(name))
            .take(count)
            .map(|n| n.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_copy_is_numbered_in_brackets_from_one() {
        assert_eq!(
            first("report.txt", 3),
            ["report (1).txt", "report (2).txt", "report (3).txt"]
        );
    }

    #[test]
    fn a_name_with_no_extension_keeps_none() {
        assert_eq!(first("Makefile", 2), ["Makefile (1)", "Makefile (2)"]);
        assert_eq!(first("Photos", 1), ["Photos (1)"]);
    }

    #[test]
    fn a_leading_dot_is_part_of_the_name_not_an_extension() {
        assert_eq!(first(".bashrc", 1), [".bashrc (1)"]);
    }

    #[test]
    fn a_numbered_copy_carries_on_counting_instead_of_nesting() {
        assert_eq!(
            first("report (1).txt", 2),
            ["report (2).txt", "report (3).txt"]
        );
        assert_eq!(first("report (41).txt", 1), ["report (42).txt"]);
    }

    #[test]
    fn brackets_that_are_not_a_copy_number_are_left_alone() {
        assert_eq!(first("draft (final).txt", 1), ["draft (final) (1).txt"]);
        assert_eq!(
            first("(1).txt", 1),
            ["(1) (1).txt"],
            "a bare number is a name"
        );
        assert_eq!(
            first("v(2).txt", 1),
            ["v(2) (1).txt"],
            "no space before the bracket"
        );
    }

    #[test]
    fn the_last_extension_is_the_extension() {
        assert_eq!(first("archive.tar.gz", 1), ["archive.tar (1).gz"]);
    }

    #[test]
    fn the_candidates_end_rather_than_running_forever() {
        assert_eq!(
            copy_names(OsStr::new("a.txt")).count(),
            MAX_COPY_NUMBER as usize
        );
        let last = copy_names(OsStr::new("a (998).txt")).collect::<Vec<_>>();
        assert_eq!(last, [OsString::from("a (999).txt")]);
        assert_eq!(
            first("a (999).txt", 1),
            ["a (999) (1).txt"],
            "the last number still has a copy"
        );
    }
}
