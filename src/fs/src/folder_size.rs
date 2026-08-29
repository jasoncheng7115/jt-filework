//! Folder sizes, measured on demand.
//!
//! A file manager that totals every folder as you arrive walks the whole disk
//! to draw one screen, so the size column is blank for folders until somebody
//! asks. This is that measurement, and the cache that keeps it from being
//! repeated.
//!
//! Iterative rather than recursive: a directory tree is untrusted input, and
//! a symlink loop or a pathologically deep tree must not consume the stack
//! (`AGENTS.md` §20.2, `docs/SECURITY.md` §13).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use jtf_jobs::CancellationToken;

/// How deep a measurement will walk.
///
/// The same bound the search walker uses. A tree deeper than this is either a
/// mistake or a loop, and either way the answer is to stop.
pub const MAX_DEPTH: usize = 64;

/// How many folders' sizes are remembered.
///
/// Bounded because it grows with every folder the user measures, and an
/// unbounded map in a long-running window is a leak with a slow fuse.
pub const MAX_CACHED: usize = 4096;

/// How long a measurement is trusted when nothing looks changed.
///
/// A folder's own modification time changes when an entry is added or removed
/// directly inside it, so that alone catches most edits and is checked first.
/// It does *not* change when a file three levels down grows, which is exactly
/// how a build directory behaves — so a time limit backs it up. Ten minutes
/// is long enough that walking around the tree never re-measures, and short
/// enough that a number nobody would otherwise question corrects itself.
pub const FRESH_FOR: Duration = Duration::from_secs(600);

/// What a completed measurement found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderSize {
    /// Total bytes of every file beneath the folder.
    pub bytes: u64,
    /// How many files were counted.
    pub files: u64,
    /// How many directories were entered.
    pub folders: u64,
    /// Whether the walk stopped early, at the depth bound or on cancellation.
    ///
    /// Reported rather than hidden: a total that silently omits part of the
    /// tree is worse than one labelled incomplete.
    pub partial: bool,
}

impl FolderSize {
    const fn empty() -> Self {
        Self {
            bytes: 0,
            files: 0,
            folders: 0,
            partial: false,
        }
    }
}

/// Measure `root`, stopping early if `cancel` is triggered.
///
/// Symlinks are never followed — `symlink_metadata`, like everywhere else in
/// this crate — so a link into a parent cannot make the walk loop, and a link
/// to a huge file elsewhere is not counted as if it lived here.
pub fn measure(root: &Path, cancel: &CancellationToken) -> FolderSize {
    let mut total = FolderSize::empty();
    // (path, depth). An explicit stack, so depth costs heap and not stack.
    let mut pending: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = pending.pop() {
        if cancel.is_cancelled() {
            total.partial = true;
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            // An unreadable directory is a permissions fact, not a failure of
            // the measurement: count what we can and say the total is partial.
            total.partial = true;
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                total.partial = true;
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                total.folders += 1;
                if depth + 1 > MAX_DEPTH {
                    total.partial = true;
                    continue;
                }
                pending.push((entry.path(), depth + 1));
            } else {
                total.files += 1;
                total.bytes = total.bytes.saturating_add(meta.len());
            }
        }
    }
    total
}

/// A remembered measurement.
#[derive(Debug, Clone, Copy)]
struct Cached {
    size: FolderSize,
    measured_at: Instant,
    /// The folder's own modification time when it was measured.
    folder_mtime: Option<SystemTime>,
}

/// Measurements, kept so that walking away and back does not re-measure.
#[derive(Debug, Default)]
pub struct SizeCache {
    entries: HashMap<PathBuf, Cached>,
}

impl SizeCache {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// The remembered size for `path`, if it is still trustworthy.
    pub fn get(&self, path: &Path) -> Option<FolderSize> {
        let cached = self.entries.get(path)?;
        if cached.measured_at.elapsed() > FRESH_FOR {
            return None;
        }
        // A changed folder mtime means an entry was added or removed directly
        // inside it, which is the cheap half of "has this changed".
        if current_mtime(path) != cached.folder_mtime {
            return None;
        }
        Some(cached.size)
    }

    /// Remember a measurement.
    pub fn insert(&mut self, path: PathBuf, size: FolderSize) {
        if self.entries.len() >= MAX_CACHED {
            // Cleared wholesale rather than evicted one at a time: the cache
            // is an optimisation, and the cost of being wrong about which
            // entry to drop is another walk, not an error.
            self.entries.clear();
        }
        let folder_mtime = current_mtime(&path);
        self.entries.insert(
            path,
            Cached {
                size,
                measured_at: Instant::now(),
                folder_mtime,
            },
        );
    }

    /// Forget `path`, so the next request measures again.
    pub fn invalidate(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// How many measurements are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn current_mtime(path: &Path) -> Option<SystemTime> {
    fs::symlink_metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-folder-size-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn write(path: &Path, bytes: usize) {
        let mut file = File::create(path).expect("create");
        file.write_all(&vec![b'x'; bytes]).expect("write");
    }

    #[test]
    fn it_totals_every_file_beneath_the_folder() {
        let root = temp_dir("total");
        write(&root.join("a"), 100);
        fs::create_dir(root.join("deep")).expect("mkdir");
        write(&root.join("deep").join("b"), 250);

        let size = measure(&root, &CancellationToken::never());
        assert_eq!(size.bytes, 350);
        assert_eq!(size.files, 2);
        assert_eq!(size.folders, 1);
        assert!(!size.partial);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_folder_is_zero_rather_than_unknown() {
        let root = temp_dir("empty");
        let size = measure(&root, &CancellationToken::never());
        assert_eq!(size.bytes, 0);
        assert!(!size.partial, "nothing failed, so nothing is missing");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelling_reports_a_partial_total_rather_than_a_wrong_one() {
        let root = temp_dir("cancel");
        write(&root.join("a"), 10);
        let (token, canceller) = CancellationToken::new();
        canceller.cancel();
        let size = measure(&root, &token);
        assert!(
            size.partial,
            "a total that stopped early must say so; a number that looks \
             complete and is not is worse than no number"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_cache_returns_what_was_measured() {
        let root = temp_dir("cache");
        write(&root.join("a"), 42);
        let size = measure(&root, &CancellationToken::never());

        let mut cache = SizeCache::new();
        cache.insert(root.clone(), size);
        assert_eq!(cache.get(&root), Some(size));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn adding_a_file_invalidates_the_cached_size() {
        let root = temp_dir("mtime");
        write(&root.join("a"), 10);
        let mut cache = SizeCache::new();
        cache.insert(root.clone(), measure(&root, &CancellationToken::never()));
        assert!(cache.get(&root).is_some());

        // Creating an entry changes the folder's own modification time.
        write(&root.join("b"), 10);
        assert_eq!(
            cache.get(&root),
            None,
            "the folder changed directly, so the remembered total is stale"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_measurement_for_a_folder_that_is_gone_is_not_trusted() {
        let root = temp_dir("gone");
        let mut cache = SizeCache::new();
        cache.insert(root.clone(), measure(&root, &CancellationToken::never()));
        let _ = fs::remove_dir_all(&root);
        assert_eq!(cache.get(&root), None);
    }

    #[test]
    fn the_cache_is_bounded() {
        let mut cache = SizeCache::new();
        for i in 0..(MAX_CACHED + 8) {
            cache.insert(PathBuf::from(format!("/nowhere/{i}")), FolderSize::empty());
        }
        assert!(cache.len() <= MAX_CACHED);
    }
}
