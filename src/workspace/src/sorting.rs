//! Sorting a pane's rows.
//!
//! Lives here rather than in the UI so that the application and the benchmark
//! measure the same code. A benchmark that measures its own reimplementation
//! measures nothing.

use jtf_core::FileEntry;

use crate::view::{SortKey, SortSpec};

/// Sort a pane's rows.
///
/// Decorate-sort-undecorate, not a clever comparator. Computing a lowercased
/// name inside the comparison costs two allocations every time two entries are
/// compared: for a 100 000-entry directory that is roughly 3.4 million
/// allocations and it measured at 226 ms, against a 250 ms budget
/// (`docs/TESTING.md` §8.2). Computing each key once costs 100 000.
///
/// With `folders_first`, directories sort ahead of files in both directions —
/// what every desktop file manager does — by partitioning and sorting the two
/// groups independently rather than reversing the whole list. Without it,
/// everything sorts together, which is what someone sorting by date to see
/// what changed actually wants.
pub fn sort_entries_with(entries: &mut Vec<FileEntry>, sort: SortSpec, folders_first: bool) {
    let (mut directories, mut files): (Vec<FileEntry>, Vec<FileEntry>) = std::mem::take(entries)
        .into_iter()
        .partition(|e| folders_first && e.kind().is_directory_on_disk());

    for group in [&mut directories, &mut files] {
        match sort.key {
            SortKey::Size => group.sort_by_cached_key(|e| e.size().unwrap_or(0)),
            SortKey::Modified => group.sort_by_cached_key(|e| e.timestamps().modified),
            SortKey::Created => group.sort_by_cached_key(|e| e.timestamps().created),
            SortKey::Accessed => group.sort_by_cached_key(|e| e.timestamps().accessed),
            SortKey::Kind | SortKey::Extension => {
                group.sort_by_cached_key(FileEntry::extension_hint);
            }
            SortKey::Name => {
                group.sort_by_cached_key(|e| natural_key(&e.display_name()));
            }
        }
        if !sort.ascending {
            group.reverse();
        }
    }

    entries.reserve(directories.len() + files.len());
    entries.append(&mut directories);
    entries.append(&mut files);
}

/// The width every run of digits is padded to.
///
/// Nineteen digits holds any `u64`, which is past every real file name and
/// well past what anyone numbers a folder with. A longer run than this is
/// left as text: it is not a quantity, it is an id, and comparing two ids by
/// magnitude is not more correct than comparing them character by character.
const DIGITS: usize = 19;

/// A sort key that orders digits by value rather than by character.
///
/// `1, 10, 11, 2, 3` is what plain text comparison gives, because `'1' < '2'`
/// and the comparison never gets to the second character. Finder, Windows
/// Explorer, GNOME Files and Dolphin all order those `1, 2, 3, 10, 11`, and
/// the user comparing the two lists side by side is right that ours is the
/// odd one out. So this is not a preference to expose - it is the behaviour
/// every file manager on all three platforms already has.
///
/// Done by padding rather than by a comparator that walks two names in step:
/// the sort is `sort_by_cached_key` for the reason written above, and a key
/// has to be a value that compares correctly on its own. Each run of digits
/// is zero-padded to a fixed width, so ordinary lexicographic comparison of
/// the padded text *is* comparison by value, and the whole key stays one
/// string with one allocation - the same cost as the `to_lowercase` it
/// replaces.
///
/// The NUL before each number puts numbers ahead of text at the same
/// position, which is where every one of those file managers puts them, and
/// it cannot collide with anything in the name because no filesystem this
/// program supports lets a name contain NUL.
pub fn natural_key(name: &str) -> String {
    // Room for the name, plus padding for a couple of numbers in it. A name
    // with more numbers than that grows the string once or twice more, which
    // is cheaper than measuring first.
    let mut key = String::with_capacity(name.len() + DIGITS * 2);
    let mut rest = name;

    while !rest.is_empty() {
        let digits_at = rest.find(|c: char| c.is_ascii_digit());
        let Some(start) = digits_at else {
            push_folded(&mut key, rest);
            break;
        };
        push_folded(&mut key, &rest[..start]);

        let tail = &rest[start..];
        let end = tail
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(tail.len());
        let (run, remainder) = tail.split_at(end);
        rest = remainder;

        // Leading zeros are not part of the value; `007` and `7` are the same
        // number and sort together, and the stable sort then keeps them in
        // the order they arrived in.
        let value = run.trim_start_matches('0');
        if value.len() > DIGITS {
            // Too long to be a quantity. Left as it is written, so that two
            // such runs still compare against each other consistently.
            push_folded(&mut key, run);
            continue;
        }
        key.push('\0');
        for _ in 0..(DIGITS - value.len()) {
            key.push('0');
        }
        key.push_str(value);
    }

    key
}

/// Append text case-folded, without allocating a second string for it.
fn push_folded(key: &mut String, text: &str) {
    for c in text.chars() {
        for folded in c.to_lowercase() {
            key.push(folded);
        }
    }
}

/// Sort with folders first, the default.
pub fn sort_entries(entries: &mut Vec<FileEntry>, sort: SortSpec) {
    sort_entries_with(entries, sort, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use jtf_core::{FileKind, Location, RawName};

    fn entry(name: &str, kind: FileKind) -> FileEntry {
        FileEntry::new(
            Location::local(format!("/tmp/{name}")),
            RawName::new(name),
            kind,
        )
    }

    fn names(entries: &[FileEntry]) -> Vec<String> {
        entries.iter().map(FileEntry::display_name).collect()
    }

    #[test]
    fn numbered_folders_sort_the_way_every_file_manager_sorts_them() {
        // The user's own folder, which Finder and Explorer both order this
        // way and we did not: we gave 1, 10, 11, 12, 13, 2, 3.
        let mut entries = vec![
            entry("1 公司介紹資料", FileKind::Directory),
            entry("10 硬體產品", FileKind::Directory),
            entry("11 交貨單", FileKind::Directory),
            entry("13收到折讓單", FileKind::Directory),
            entry("2 廠商資料表", FileKind::Directory),
            entry("9 收到發票", FileKind::Directory),
        ];
        sort_entries_with(
            &mut entries,
            SortSpec {
                key: SortKey::Name,
                ascending: true,
            },
            true,
        );
        assert_eq!(
            names(&entries),
            [
                "1 公司介紹資料",
                "2 廠商資料表",
                "9 收到發票",
                "10 硬體產品",
                "11 交貨單",
                "13收到折讓單",
            ]
        );
    }

    /// `natural_key` on its own, because the interesting cases are all about
    /// where a number starts and stops rather than about sorting.
    fn ordered(names: &[&str]) -> Vec<String> {
        let mut keyed: Vec<&str> = names.to_vec();
        keyed.sort_by_key(|n| natural_key(n));
        keyed.into_iter().map(str::to_owned).collect()
    }

    #[test]
    fn a_number_anywhere_in_the_name_counts_not_just_at_the_front() {
        assert_eq!(
            ordered(&["img_10.jpg", "img_9.jpg", "img_100.jpg"]),
            ["img_9.jpg", "img_10.jpg", "img_100.jpg"]
        );
        assert_eq!(
            ordered(&["v1.10.0", "v1.9.0", "v1.2.30"]),
            ["v1.2.30", "v1.9.0", "v1.10.0"],
            "every run of digits is compared by value, not only the first"
        );
    }

    #[test]
    fn numbers_come_before_text_and_case_still_does_not_matter() {
        assert_eq!(ordered(&["Beta", "2nd", "alpha"]), ["2nd", "alpha", "Beta"]);
    }

    #[test]
    fn leading_zeros_do_not_change_the_value() {
        assert_eq!(
            ordered(&["file007", "file8", "file06"]),
            ["file06", "file007", "file8"]
        );
    }

    #[test]
    fn a_run_too_long_to_be_a_quantity_is_left_as_text() {
        // Twenty digits and more: an id, not a number anyone is counting
        // with. It must still sort consistently against its own kind.
        let long = "a12345678901234567890";
        let longer = "a12345678901234567891";
        assert_eq!(ordered(&[longer, long]), [long, longer]);
        assert_eq!(natural_key(long), long, "an id was padded as if it were a count");
    }

    #[test]
    fn a_name_that_is_only_digits_and_one_with_none_both_work() {
        assert_eq!(ordered(&["10", "9", "abc", ""]), ["", "9", "10", "abc"]);
    }

    #[test]
    fn folders_first_groups_them_ahead_in_both_directions() {
        let make = || {
            vec![
                entry("b.txt", FileKind::File),
                entry("a-dir", FileKind::Directory),
                entry("a.txt", FileKind::File),
                entry("z-dir", FileKind::Directory),
            ]
        };
        let sort = SortSpec {
            key: SortKey::Name,
            ascending: true,
        };

        let mut ascending = make();
        sort_entries_with(&mut ascending, sort, true);
        assert_eq!(names(&ascending), ["a-dir", "z-dir", "a.txt", "b.txt"]);

        let mut descending = make();
        sort_entries_with(
            &mut descending,
            SortSpec {
                ascending: false,
                ..sort
            },
            true,
        );
        assert_eq!(
            names(&descending),
            ["z-dir", "a-dir", "b.txt", "a.txt"],
            "folders stay ahead when the direction flips"
        );
    }

    #[test]
    fn without_folders_first_everything_sorts_together() {
        let mut entries = vec![
            entry("b.txt", FileKind::File),
            entry("a-dir", FileKind::Directory),
            entry("a.txt", FileKind::File),
            entry("z-dir", FileKind::Directory),
        ];
        sort_entries_with(
            &mut entries,
            SortSpec {
                key: SortKey::Name,
                ascending: true,
            },
            false,
        );
        assert_eq!(names(&entries), ["a-dir", "a.txt", "b.txt", "z-dir"]);
    }

    #[test]
    fn the_default_puts_folders_first() {
        let mut entries = vec![
            entry("a.txt", FileKind::File),
            entry("z-dir", FileKind::Directory),
        ];
        sort_entries(&mut entries, SortSpec::default());
        assert_eq!(names(&entries), ["z-dir", "a.txt"]);
    }
}
