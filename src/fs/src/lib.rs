//! Filesystem providers.
//!
//! A provider turns a [`Location`](jtf_core::Location) into
//! [`FileEntry`](jtf_core::FileEntry) rows. The local filesystem is one
//! provider; archives and search results will be others, which is why this is
//! a trait rather than a function.
//!
//! # Never on the UI thread
//!
//! `AGENTS.md` §3 forbids directory enumeration on the UI thread, and means
//! it: a directory on a stalled network mount can block for minutes. So the
//! interesting API here is [`enumerate_async`], which delivers rows in
//! batches, honours a [`CancellationToken`](jtf_jobs::CancellationToken) at
//! every entry, and can be abandoned without waiting.
//!
//! [`Provider::list`] exists for tests and for callers that genuinely have a
//! small, local, known-good directory. It is not the normal path.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod archive;
pub mod compare;
mod folder_size;
pub mod iso;
mod local;
mod provider;
pub mod sftp;
pub mod tarball;
pub mod usage;

pub use archive::{
    create as create_archive, extract as extract_archive,
    extract_members as extract_archive_members, Extracted,
};
pub use compare::{
    compare, compare_with, Comparison, Difference, Progress as CompareProgress,
    Row as ComparisonRow, Side as ComparisonSide,
};
pub use folder_size::{
    measure, measure_with, FolderSize, SizeCache, FRESH_FOR, MAX_CACHED, MAX_DEPTH,
};
pub use iso::{extract as extract_iso, extract_members as extract_iso_members};

/// Extract from whichever container `path` is.
///
/// One entry point, so `Z` on a file and `C` inside its window take the same
/// route whatever the container turns out to be.
///
/// # Errors
///
/// Whatever the reader for that container reports, and
/// [`jtf_core::ErrorCode::Unsupported`] when the file is not one this build
/// can open.
/// Every entry in whichever container `path` is.
///
/// One entry point, so the archive window does not have to know which format
/// it opened. Tar members come back in the same `ArchiveEntry` shape the ZIP
/// and ISO listings produce, so the window did not change.
///
/// # Errors
///
/// Whatever the reader for that container reports, and
/// [`jtf_core::ErrorCode::Unsupported`] when the file is not one this build
/// can open.
pub fn list_container(path: &std::path::Path) -> jtf_core::Result<Vec<jtf_viewer::ArchiveEntry>> {
    match jtf_viewer::container_of(path) {
        Some(jtf_viewer::Container::Tar) => Ok(tarball::list(path)?
            .into_iter()
            .map(|entry| jtf_viewer::ArchiveEntry {
                name: entry.name,
                size: entry.size,
                // A tar header does not record a compressed size per member:
                // the compression wraps the whole stream, not each entry.
                compressed_size: 0,
                is_directory: entry.is_directory,
                unsafe_name: entry.unsafe_name,
            })
            .collect()),
        _ => jtf_viewer::list_container(path),
    }
}

/// Extract from whichever container `path` is.
///
/// One entry point, so `Z` on a file and `C` inside its window take the same
/// route whatever the container turns out to be.
///
/// # Errors
///
/// Whatever the reader for that container reports, and
/// [`jtf_core::ErrorCode::Unsupported`] when the file is not one this build
/// can open.
pub fn extract_container_members(
    container: &std::path::Path,
    destination: &std::path::Path,
    wanted: &[String],
    cancel: &jtf_jobs::CancellationToken,
    progress: impl FnMut(&archive::Extracted),
) -> jtf_core::Result<archive::Extracted> {
    match jtf_viewer::container_of(container) {
        Some(jtf_viewer::Container::Zip) => {
            archive::extract_members(container, destination, wanted, cancel, progress)
        }
        Some(jtf_viewer::Container::Iso) => {
            iso::extract_members(container, destination, wanted, cancel, progress)
        }
        Some(jtf_viewer::Container::Tar) => {
            tarball::extract_members(container, destination, wanted, cancel, progress)
        }
        None => Err(jtf_core::Error::new(
            jtf_core::ErrorCode::Unsupported,
            "not a container this build can open",
        )),
    }
}
pub use local::LocalProvider;
pub use provider::{Batch, EnumerationHandle, Provider};
pub use sftp::{verify as verify_host_key, HostKeyVerdict, SftpProvider};
pub use tarball::{
    create as create_tarball, extract_members as extract_tarball_members, kind_of as tar_kind,
    Compression, Kind as TarKind, TarEntry,
};
pub use usage::{
    analyse as analyse_usage, analyse_with as analyse_usage_with, FolderUsage, KindUsage,
    Progress as UsageProgress, Usage,
};
