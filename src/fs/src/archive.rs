//! Extracting and creating ZIP archives.
//!
//! `docs/adr/0003-archive-extraction.md`: reading a listing and taking things
//! out are different jobs with different risks. The listing lives in
//! `jtf-viewer` and touches no decompressor at all; this is the part that
//! does, and every rule here exists because a member name is written by
//! whoever made the archive.
//!
//! What this refuses, always:
//!
//! - a member whose resolved destination is not inside the chosen folder,
//!   however it is spelled — `../`, an absolute path, a Windows drive letter,
//!   a UNC prefix, or a name that only escapes once the platform has
//!   normalised it;
//! - a symbolic link member, because following one at extraction time is how
//!   a later write lands outside the tree;
//! - an archive that expands past a bound, so one that claims to be 4 KB and
//!   unpacks to 40 GB stops rather than filling the disk.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use jtf_core::{Error, ErrorCode, Result};
use jtf_jobs::CancellationToken;

/// The largest a single member may expand to.
///
/// Generous for real files, small enough that a zip bomb is stopped long
/// before the disk is.
pub const MAX_MEMBER_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// The largest an archive may expand to in total.
pub const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// Copied in this many bytes at a time, so cancellation is noticed promptly
/// and a huge member never becomes a huge allocation.
const CHUNK: usize = 64 * 1024;

/// What an extraction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Extracted {
    /// Members written.
    pub files: u64,
    /// Directories created.
    pub folders: u64,
    /// Bytes written.
    pub bytes: u64,
    /// Members refused because their name would have escaped, or because they
    /// were links. Reported, never silently skipped.
    pub refused: u64,
}

/// Where a member would land, or `None` if it would escape `destination`.
///
/// The check is on the *resolved* path, not on the spelling: a name is
/// rejected because of where it ends up, which is the only question that
/// matters and the only one that cannot be fooled by a new way of writing
/// `..`.
pub(crate) fn safe_destination(destination: &Path, name: &str) -> Option<PathBuf> {
    // A backslash is a separator on Windows and a legal filename character on
    // Unix. Treated as a separator either way: an archive built on Windows
    // that says `a\..\..\b` must not become a single strange filename here
    // and a traversal there.
    let normalised = name.replace('\\', "/");

    let mut out = destination.to_path_buf();
    for part in normalised.split('/') {
        match Path::new(part).components().next() {
            // Nothing to add: an empty part from a doubled or trailing slash,
            // or a `.` that means "here".
            None | Some(Component::CurDir) => {}
            Some(Component::Normal(segment)) => out.push(segment),
            // Anything else is a way out: `..`, a root, or a Windows prefix
            // like `C:` or `\\server\share`.
            Some(_) => return None,
        }
    }
    // Nothing was added, so the name was `.` or empty.
    if out == destination {
        return None;
    }
    Some(out)
}

/// Extract every member of the ZIP at `archive` into `destination`.
///
/// `progress` is called with the running byte count.
///
/// # Errors
///
/// [`ErrorCode::Io`] if the archive cannot be read or the destination cannot
/// be written; [`ErrorCode::ParseFailed`] if it is not a ZIP;
/// [`ErrorCode::Cancelled`] if the token was triggered;
/// [`ErrorCode::LimitExceeded`] if the archive expands past the bounds above.
pub fn extract(
    archive: &Path,
    destination: &Path,
    cancel: &CancellationToken,
    progress: impl FnMut(&Extracted),
) -> Result<Extracted> {
    extract_members(archive, destination, &[], cancel, progress)
}

/// Extract only the named members, or everything when `wanted` is empty.
///
/// `CV.HLP` §四 gives `C` as 拷貝(解壓)檔案 - the marked members - and `X` as
/// all of them. The same walk serves both; the filter is on the name as
/// stored, before any path resolution, because that is the name the listing
/// showed and the one the user picked from.
///
/// # Errors
///
/// As [`extract`].
pub fn extract_members(
    archive: &Path,
    destination: &Path,
    wanted: &[String],
    cancel: &CancellationToken,
    mut progress: impl FnMut(&Extracted),
) -> Result<Extracted> {
    let file =
        File::open(archive).map_err(|e| Error::new(ErrorCode::Io, format!("open archive: {e}")))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("not a readable zip: {e}")))?;

    fs::create_dir_all(destination)
        .map_err(|e| Error::new(ErrorCode::Io, format!("create destination: {e}")))?;

    let mut done = Extracted::default();
    for index in 0..zip.len() {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let mut member = zip
            .by_index(index)
            .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("member {index}: {e}")))?;

        // Not asked for. Skipped before any of the checks below, so a member
        // nobody picked is neither written nor counted as refused - it was
        // never a candidate.
        if !wanted.is_empty() && !wanted.iter().any(|name| name == member.name()) {
            continue;
        }

        // `enclosed_name` is the crate's own traversal check; ours runs as
        // well rather than instead, because the two disagreeing is exactly
        // the case worth refusing.
        let Some(target) = member
            .enclosed_name()
            .and_then(|_| safe_destination(destination, member.name()))
        else {
            done.refused += 1;
            continue;
        };

        if member.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
            done.folders += 1;
            continue;
        }
        // A link member is refused rather than created: writing one now means
        // a later write through it lands wherever the archive chose.
        if member.is_symlink() {
            done.refused += 1;
            continue;
        }

        if member.size() > MAX_MEMBER_BYTES {
            return Err(Error::new(
                ErrorCode::LimitExceeded,
                format!("a member claims {} bytes", member.size()),
            ));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
        }
        let mut out = File::create(&target)
            .map_err(|e| Error::new(ErrorCode::Io, format!("create file: {e}")))?;

        // Copied by hand rather than with `io::copy`, so the bound is checked
        // against what actually arrives and not against what the header
        // claimed - a lying header is the whole point of a zip bomb.
        let mut buffer = vec![0_u8; CHUNK];
        loop {
            if cancel.is_cancelled() {
                // The partial file goes: a cancelled extraction should not
                // leave something that looks extracted.
                drop(out);
                let _ = fs::remove_file(&target);
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            let read = match member.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::new(ErrorCode::Io, format!("read member: {e}"))),
            };
            done.bytes += read as u64;
            if done.bytes > MAX_TOTAL_BYTES {
                drop(out);
                let _ = fs::remove_file(&target);
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "the archive expands past what this will unpack",
                ));
            }
            out.write_all(&buffer[..read])
                .map_err(|e| Error::new(ErrorCode::Io, format!("write: {e}")))?;
        }
        done.files += 1;
        progress(&done);
    }
    Ok(done)
}

/// Create a ZIP at `archive` holding `sources`.
///
/// Directories are added with their contents. Symlinks are skipped rather
/// than followed, for the same reason extraction refuses them.
///
/// # Errors
///
/// [`ErrorCode::Io`] on any read or write failure;
/// [`ErrorCode::Cancelled`] if the token was triggered.
pub fn create(
    archive: &Path,
    sources: &[PathBuf],
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64),
) -> Result<u64> {
    let file = File::create(archive)
        .map_err(|e| Error::new(ErrorCode::Io, format!("create archive: {e}")))?;
    let mut writer = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut added = 0_u64;
    for source in sources {
        let base = source.parent().unwrap_or(Path::new(""));
        let mut pending = vec![source.clone()];
        while let Some(path) = pending.pop() {
            if cancel.is_cancelled() {
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            let meta = fs::symlink_metadata(&path)
                .map_err(|e| Error::new(ErrorCode::Io, format!("stat: {e}")))?;
            if meta.is_symlink() {
                continue;
            }
            // The name inside the archive is relative to what was selected, so
            // a selection of `/a/b/c` stores `c/...` and never `/a/b/c/...`.
            let stored = path.strip_prefix(base).unwrap_or(&path);
            let name = stored.to_string_lossy().replace('\\', "/");
            if meta.is_dir() {
                writer
                    .add_directory(format!("{name}/"), options)
                    .map_err(|e| Error::new(ErrorCode::Io, format!("add directory: {e}")))?;
                let entries = fs::read_dir(&path)
                    .map_err(|e| Error::new(ErrorCode::Io, format!("read directory: {e}")))?;
                for entry in entries.flatten() {
                    pending.push(entry.path());
                }
                continue;
            }
            writer
                .start_file(name, options)
                .map_err(|e| Error::new(ErrorCode::Io, format!("add file: {e}")))?;
            let mut input =
                File::open(&path).map_err(|e| Error::new(ErrorCode::Io, format!("open: {e}")))?;
            io::copy(&mut input, &mut writer)
                .map_err(|e| Error::new(ErrorCode::Io, format!("compress: {e}")))?;
            added += 1;
            progress(added);
        }
    }
    writer
        .finish()
        .map_err(|e| Error::new(ErrorCode::Io, format!("finish archive: {e}")))?;
    Ok(added)
}
