//! The local filesystem provider.
//!
//! "Local" means "reachable through the platform's normal file API" — which
//! includes network mounts, and that is exactly why every loop in here checks
//! for cancellation. An SMB share that stops answering must not turn into a
//! thread that cannot be stopped.

use std::ffi::OsStr;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use jtf_core::file::{Attributes, PermissionsSummary, Timestamps};
use jtf_core::{Error, ErrorCode, FileEntry, FileKind, Location, RawName, Result};
use jtf_jobs::CancellationToken;

use crate::provider::{Batch, EnumerationHandle, Provider};

/// How many rows accumulate before a batch is sent.
///
/// Small enough that the first screenful appears immediately on a huge
/// directory; large enough that a million entries do not become a million
/// channel sends.
const BATCH_SIZE: usize = 256;

/// Extensions treated as archives, so the UI can offer to look inside and the
/// security rules for untrusted containers apply (`docs/SECURITY.md` §4).
const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "tar", "jar", "war", "iso", "dmg", "cab",
];

/// Directory extensions macOS presents as a single item.
const BUNDLE_EXTENSIONS: &[&str] = &["app"];
const PACKAGE_EXTENSIONS: &[&str] = &[
    "bundle",
    "framework",
    "kext",
    "plugin",
    "rtfd",
    "pages",
    "numbers",
    "key",
    "photoslibrary",
];

/// Lists real directories.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalProvider;

impl LocalProvider {
    /// A provider.
    pub const fn new() -> Self {
        Self
    }

    /// Build an entry from a directory entry, without following symlinks.
    ///
    /// `symlink_metadata` is deliberate: a symlink must be reported as a
    /// symlink, not silently as whatever it points at
    /// (`docs/SECURITY.md` §3.1).
    fn entry_from(dir_entry: &DirEntry) -> FileEntry {
        let path = dir_entry.path();
        let raw_name = RawName::new(dir_entry.file_name());
        let link_meta = fs::symlink_metadata(&path);
        let kind = classify(&path, link_meta.as_ref().ok());

        let mut entry = FileEntry::new(Location::local(path.clone()), raw_name, kind);

        if let Ok(meta) = &link_meta {
            if !meta.is_dir() {
                entry = entry.with_size(meta.len());
            }
            entry = entry
                .with_timestamps(timestamps_of(meta))
                .with_attributes(attributes_of(&path, meta))
                .with_permissions(permissions_of(meta));
        }
        entry
    }
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
}

fn classify(path: &Path, meta: Option<&Metadata>) -> FileKind {
    let Some(meta) = meta else {
        return FileKind::Unknown;
    };
    let file_type = meta.file_type();

    if file_type.is_symlink() {
        return FileKind::Symlink;
    }
    if file_type.is_dir() {
        return match extension_lower(path).as_deref() {
            Some(ext) if BUNDLE_EXTENSIONS.contains(&ext) => FileKind::ApplicationBundle,
            Some(ext) if PACKAGE_EXTENSIONS.contains(&ext) => FileKind::Package,
            _ => FileKind::Directory,
        };
    }
    if file_type.is_file() {
        return match extension_lower(path).as_deref() {
            Some(ext) if ARCHIVE_EXTENSIONS.contains(&ext) => FileKind::Archive,
            _ => FileKind::File,
        };
    }
    FileKind::Device
}

fn timestamps_of(meta: &Metadata) -> Timestamps {
    Timestamps {
        modified: meta.modified().ok(),
        created: meta.created().ok(),
        accessed: meta.accessed().ok(),
        // Distinguished from `modified` only on some platforms; the platform
        // adapter fills it in where it is meaningful.
        metadata_changed: None,
    }
}

fn attributes_of(path: &Path, meta: &Metadata) -> Attributes {
    let hidden = path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.starts_with('.'));

    Attributes {
        hidden,
        read_only: meta.permissions().readonly(),
        // The platform hidden flag, system flag and cloud-placeholder state
        // need platform APIs and belong to the platform adapters
        // (docs/PLATFORM_INTEGRATION.md 1). Reporting a guess here would be
        // worse than reporting nothing.
        system: false,
        cloud_placeholder: false,
    }
}

fn permissions_of(meta: &Metadata) -> PermissionsSummary {
    let writable = !meta.permissions().readonly();
    PermissionsSummary {
        // A cross-platform metadata read cannot answer these honestly.
        // `readable` is optimistic and corrected the moment an operation
        // actually fails; `executable` needs the unix mode or the Windows
        // equivalent and is filled in by the platform adapter.
        readable: true,
        writable,
        executable: false,
    }
}

/// Map an I/O failure to a stable code.
fn map_io(error: &io::Error, context: &Path) -> Error {
    let code = match error.kind() {
        io::ErrorKind::NotFound => ErrorCode::NotFound,
        io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
        io::ErrorKind::TimedOut => ErrorCode::TimedOut,
        io::ErrorKind::NotADirectory => ErrorCode::WrongKind,
        _ => ErrorCode::Io,
    };
    Error::new(code, format!("{}: {error}", context.display()))
}

impl Provider for LocalProvider {
    fn handles(&self, location: &Location) -> bool {
        location.as_path().is_some()
    }

    fn list(&self, location: &Location, cancel: &CancellationToken) -> Result<Vec<FileEntry>> {
        let path = location
            .as_path()
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "not a local location"))?;

        let read_dir = fs::read_dir(path).map_err(|e| map_io(&e, path))?;
        let mut out = Vec::new();
        for dir_entry in read_dir {
            if cancel.is_cancelled() {
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            // One unreadable entry must not lose the other 99 999.
            if let Ok(dir_entry) = dir_entry {
                out.push(Self::entry_from(&dir_entry));
            }
        }
        Ok(out)
    }

    fn enumerate_async(&self, location: &Location) -> Result<EnumerationHandle> {
        let path = location
            .as_path()
            .ok_or_else(|| Error::new(ErrorCode::Unsupported, "not a local location"))?
            .to_path_buf();

        let (token, canceller) = CancellationToken::new();
        let (sender, receiver) = mpsc::channel();

        let join = thread::Builder::new()
            .name("jtf-enumerate".to_string())
            .spawn(move || {
                let read_dir = match fs::read_dir(&path) {
                    Ok(read_dir) => read_dir,
                    Err(error) => {
                        let _ = sender.send(Batch::Failed(map_io(&error, &path)));
                        return;
                    }
                };

                let mut buffer = Vec::with_capacity(BATCH_SIZE);
                let mut total = 0usize;

                for dir_entry in read_dir {
                    if token.is_cancelled() {
                        // Deliberately silent: a cancelled scan sends nothing
                        // further, so a pane that navigated away cannot be
                        // handed a stale result (AGENTS.md 3).
                        return;
                    }
                    let Ok(dir_entry) = dir_entry else { continue };
                    buffer.push(LocalProvider::entry_from(&dir_entry));

                    if buffer.len() >= BATCH_SIZE {
                        total += buffer.len();
                        if sender
                            .send(Batch::Rows(std::mem::take(&mut buffer)))
                            .is_err()
                        {
                            return; // receiver gone: nobody is waiting
                        }
                        buffer.reserve(BATCH_SIZE);
                    }
                }

                if !buffer.is_empty() {
                    total += buffer.len();
                    if sender.send(Batch::Rows(buffer)).is_err() {
                        return;
                    }
                }
                let _ = sender.send(Batch::Done { total });
            })
            .map_err(|e| Error::new(ErrorCode::Internal, format!("spawn enumerator: {e}")))?;

        Ok(EnumerationHandle::new(canceller, receiver, join))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A temporary directory that removes itself.
    struct Fixture {
        path: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir()
                .join(format!("jtf-fs-{}-{nanos}-{unique}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn file(&self, name: &str, bytes: &[u8]) -> &Self {
            fs::write(self.path.join(name), bytes).unwrap();
            self
        }

        fn dir(&self, name: &str) -> &Self {
            fs::create_dir_all(self.path.join(name)).unwrap();
            self
        }

        fn location(&self) -> Location {
            Location::local(&self.path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn names(entries: &[FileEntry]) -> Vec<String> {
        let mut v: Vec<_> = entries.iter().map(FileEntry::display_name).collect();
        v.sort();
        v
    }

    fn find<'a>(entries: &'a [FileEntry], name: &str) -> &'a FileEntry {
        entries
            .iter()
            .find(|e| e.display_name() == name)
            .expect("entry present")
    }

    #[test]
    fn lists_a_directory() {
        let fixture = Fixture::new();
        fixture
            .file("a.txt", b"hello")
            .dir("sub")
            .file(".hidden", b"");

        let entries = LocalProvider::new()
            .list(&fixture.location(), &CancellationToken::never())
            .unwrap();

        assert_eq!(names(&entries), vec![".hidden", "a.txt", "sub"]);
    }

    #[test]
    fn classifies_kinds_and_reports_size_only_for_files() {
        let fixture = Fixture::new();
        fixture
            .file("notes.txt", b"12345")
            .dir("folder")
            .file("bundle.zip", b"x")
            .dir("Thing.app");

        let entries = LocalProvider::new()
            .list(&fixture.location(), &CancellationToken::never())
            .unwrap();

        assert_eq!(find(&entries, "notes.txt").kind(), FileKind::File);
        assert_eq!(find(&entries, "notes.txt").size(), Some(5));
        assert_eq!(find(&entries, "folder").kind(), FileKind::Directory);
        assert_eq!(
            find(&entries, "folder").size(),
            None,
            "a directory has no meaningful size"
        );
        assert_eq!(find(&entries, "bundle.zip").kind(), FileKind::Archive);
        assert!(find(&entries, "bundle.zip").kind().is_untrusted_container());
        assert_eq!(
            find(&entries, "Thing.app").kind(),
            FileKind::ApplicationBundle
        );
        assert!(!find(&entries, "Thing.app").kind().is_navigable_by_default());
    }

    #[test]
    fn a_dot_prefixed_name_is_hidden() {
        let fixture = Fixture::new();
        fixture.file(".profile", b"").file("visible", b"");

        let entries = LocalProvider::new()
            .list(&fixture.location(), &CancellationToken::never())
            .unwrap();

        assert!(find(&entries, ".profile").attributes().hidden);
        assert!(!find(&entries, "visible").attributes().hidden);
    }

    #[test]
    fn a_missing_directory_reports_not_found_rather_than_an_empty_list() {
        let error = LocalProvider::new()
            .list(
                &Location::local("/definitely/not/here"),
                &CancellationToken::never(),
            )
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::NotFound);
    }

    #[test]
    fn a_non_local_location_is_unsupported_not_empty() {
        let error = LocalProvider::new()
            .list(
                &Location::virtual_location("search", "1"),
                &CancellationToken::never(),
            )
            .unwrap_err();
        assert_eq!(error.code(), ErrorCode::Unsupported);
    }

    #[test]
    fn async_enumeration_delivers_every_row_then_done() {
        let fixture = Fixture::new();
        for i in 0..1000 {
            fixture.file(&format!("f{i:04}.txt"), b"x");
        }

        let handle = LocalProvider::new()
            .enumerate_async(&fixture.location())
            .unwrap();

        let mut rows = 0usize;
        let mut batches = 0usize;
        let mut reported = None;
        while let Some(batch) = handle.recv() {
            match batch {
                Batch::Rows(r) => {
                    batches += 1;
                    rows += r.len();
                }
                Batch::Done { total } => {
                    reported = Some(total);
                    break;
                }
                Batch::Failed(e) => panic!("unexpected failure: {e}"),
            }
        }

        assert_eq!(rows, 1000);
        assert_eq!(reported, Some(1000));
        assert!(
            batches > 1,
            "rows must arrive incrementally, not in one lump"
        );
    }

    #[test]
    fn a_cancelled_enumeration_stops_and_sends_no_stale_result() {
        // AGENTS.md 3: stale results are rejected, not merely ignored.
        let fixture = Fixture::new();
        for i in 0..5000 {
            fixture.file(&format!("f{i:05}.txt"), b"x");
        }

        let handle = LocalProvider::new()
            .enumerate_async(&fixture.location())
            .unwrap();
        handle.cancel();

        let mut saw_done = false;
        while let Some(batch) = handle.recv() {
            if matches!(batch, Batch::Done { .. }) {
                saw_done = true;
            }
        }
        assert!(!saw_done, "a cancelled scan must not report completion");
        assert!(handle.is_cancelled());
    }

    #[test]
    fn dropping_the_handle_cancels_the_work() {
        let fixture = Fixture::new();
        for i in 0..2000 {
            fixture.file(&format!("f{i:05}.txt"), b"x");
        }

        let handle = LocalProvider::new()
            .enumerate_async(&fixture.location())
            .unwrap();
        drop(handle); // must return promptly, not after the whole scan
    }

    #[test]
    fn a_failing_directory_reports_through_the_channel_not_by_panicking() {
        let handle = LocalProvider::new()
            .enumerate_async(&Location::local("/definitely/not/here"))
            .unwrap();

        match handle.recv() {
            Some(Batch::Failed(error)) => assert_eq!(error.code(), ErrorCode::NotFound),
            other => panic!("expected a failure batch, got {other:?}"),
        }
    }

    #[test]
    fn unicode_and_awkward_names_survive() {
        let fixture = Fixture::new();
        fixture
            .file("\u{4e2d}\u{6587}\u{6a94}\u{6848}.txt", b"")
            .file("emoji \u{1f600}.md", b"")
            .file("  leading and trailing  ", b"")
            .file("dots...", b"");

        let entries = LocalProvider::new()
            .list(&fixture.location(), &CancellationToken::never())
            .unwrap();
        assert_eq!(entries.len(), 4);
        assert!(entries
            .iter()
            .any(|e| e.display_name().contains('\u{4e2d}')));
    }
}
