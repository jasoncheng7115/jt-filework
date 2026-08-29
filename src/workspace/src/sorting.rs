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
            SortKey::Kind | SortKey::Extension => {
                group.sort_by_cached_key(FileEntry::extension_hint);
            }
            SortKey::Name => {
                group.sort_by_cached_key(|e| e.display_name().to_lowercase());
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
