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
/// Directories sort first in both directions, as every desktop file manager
/// does, so the two groups are partitioned and sorted independently rather
/// than reversing the whole list.
pub fn sort_entries(entries: &mut Vec<FileEntry>, sort: SortSpec) {
    let (mut directories, mut files): (Vec<FileEntry>, Vec<FileEntry>) = std::mem::take(entries)
        .into_iter()
        .partition(|e| e.kind().is_directory_on_disk());

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
