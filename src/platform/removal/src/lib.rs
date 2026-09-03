//! Removing a file, a link or a whole tree.
//!
//! In its own crate because doing this safely needs syscalls the standard
//! library does not expose, and those are platform code (`AGENTS.md` §5).
//!
//! # Why not `std::fs::remove_dir_all`
//!
//! Every path-based removal has the same race. The program asks whether a name
//! is a directory, is told yes, and then acts on that *name* — and between the
//! two, anyone who can write in that directory can replace it with a symlink
//! somewhere else. The delete then walks out of the tree the user selected and
//! into whatever the link points at. It needs no privilege the attacker does
//! not already have; it needs write access to one subdirectory of something the
//! user is about to delete, which a shared folder, a synced folder or a second
//! account all provide.
//!
//! A file manager deleting files it was not asked to delete is the worst thing
//! this program can do, so the walk here never resolves a path twice.
//!
//! # What it does instead
//!
//! On Unix the parent directory is *opened*, and every step after that is
//! relative to that open descriptor: `openat` to descend, `statat` with
//! `SYMLINK_NOFOLLOW` to ask what something is, `unlinkat` to remove it. A
//! descriptor cannot be swapped; it refers to the directory that was opened,
//! not to whatever now answers to its name. `O_NOFOLLOW` on the descent means
//! a name that has become a symlink is an error rather than a door.
//!
//! On Windows there is no `openat`, and the equivalent (`NtCreateFile` with a
//! relative root handle) is not reachable through anything portable. That build
//! uses the path-based walk, which is what the program has always done, and
//! this comment is here so nobody believes otherwise.

use std::path::Path;

use jtf_core::{Error, ErrorCode};

/// How deep the walk will go.
///
/// The same bound the other walkers use. A tree deeper than this is either a
/// mistake or a construction, and neither is worth unbounded recursion.
pub const MAX_DEPTH: usize = 64;

/// Remove `path`: a file, a symlink, or a directory and everything in it.
///
/// A symlink is removed as a link. Nothing outside `path` is touched, whatever
/// the tree does while the walk is running.
///
/// # Errors
///
/// Whatever the system reported, with the path that failed.
pub fn remove_tree(path: &Path) -> Result<(), Error> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| io_error(path, &e))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        // A link is removed as a link: `remove_file` on a symlink unlinks the
        // link, and never touches what it points at.
        return std::fs::remove_file(path).map_err(|e| io_error(path, &e));
    }
    remove_directory(path)
}

/// Whether this build resolves each name once, or once per operation.
///
/// Reported rather than assumed, so a caller — or a test — can say which
/// guarantee it is getting instead of hoping.
pub const fn is_race_free() -> bool {
    cfg!(unix)
}

#[cfg(unix)]
fn remove_directory(path: &Path) -> Result<(), Error> {
    use rustix::fs::{Mode, OFlags};

    // The one and only time a path is resolved. Everything below is relative
    // to this descriptor.
    let dir = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| errno_error(path, e))?;

    empty_directory(&dir, path, 0)?;
    std::fs::remove_dir(path).map_err(|e| io_error(path, &e))
}

/// Remove everything inside `dir`, leaving the directory itself.
///
/// `shown` is only for error messages: it is where the walk started, not a
/// path anything is resolved against.
#[cfg(unix)]
fn empty_directory(dir: &rustix::fd::OwnedFd, shown: &Path, depth: usize) -> Result<(), Error> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};

    if depth >= MAX_DEPTH {
        return Err(Error::new(
            ErrorCode::Unsupported,
            format!(
                "{}: deeper than {MAX_DEPTH} levels; refusing to keep going",
                shown.display()
            ),
        ));
    }

    // Read the whole directory before removing anything from it. Iterating a
    // directory while unlinking out of it is defined loosely enough across
    // filesystems that entries can be skipped, and a skipped entry here means
    // a directory that will not empty and a delete that reports success it did
    // not achieve.
    let mut names = Vec::new();
    let entries = rustix::fs::Dir::read_from(dir).map_err(|e| errno_error(shown, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| errno_error(shown, e))?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        names.push(name.to_owned());
    }

    for name in names {
        // Asked of the descriptor, not of a path, and without following a
        // link: this answer cannot go stale between here and the unlink below,
        // because both name the same directory descriptor.
        let stat = rustix::fs::statat(dir, name.as_c_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| errno_error(shown, e))?;

        if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
            let child = rustix::fs::openat(
                dir,
                name.as_c_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| errno_error(shown, e))?;
            empty_directory(&child, shown, depth + 1)?;
            drop(child);
            rustix::fs::unlinkat(dir, name.as_c_str(), AtFlags::REMOVEDIR)
                .map_err(|e| errno_error(shown, e))?;
        } else {
            // Everything that is not a directory, symlinks included, goes with
            // one unlink. A symlink is removed as a link.
            rustix::fs::unlinkat(dir, name.as_c_str(), AtFlags::empty())
                .map_err(|e| errno_error(shown, e))?;
        }
    }
    Ok(())
}

/// The path-based walk, for platforms with no directory-relative syscalls.
///
/// Iterative, and removes a symlink as a link rather than descending through
/// it — the same guarantee the program has always had, without the stronger
/// one above.
#[cfg(not(unix))]
fn remove_directory(path: &Path) -> Result<(), Error> {
    use std::fs;

    let mut directories = Vec::new();
    // Depth travels with each directory, so the same bound the descriptor
    // walk enforces applies here too. Without it this build walked a tree of
    // any depth: the count bound below stops a *wide* tree and says nothing
    // about a deep one, and the two are different accidents.
    let mut stack = vec![(path.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth >= MAX_DEPTH {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!(
                    "{}: deeper than {MAX_DEPTH} levels; refusing to keep going",
                    dir.display()
                ),
            ));
        }
        if directories.len() > 1_000_000 {
            return Err(Error::new(
                ErrorCode::Unsupported,
                format!("{}: too many directories to remove", path.display()),
            ));
        }
        directories.push(dir.clone());
        for entry in fs::read_dir(&dir).map_err(|e| io_error(&dir, &e))? {
            let entry = entry.map_err(|e| io_error(&dir, &e))?;
            let meta = entry
                .metadata()
                .or_else(|_| fs::symlink_metadata(entry.path()))
                .map_err(|e| io_error(&entry.path(), &e))?;
            if meta.is_dir() && !meta.file_type().is_symlink() {
                stack.push((entry.path(), depth + 1));
            } else {
                fs::remove_file(entry.path()).map_err(|e| io_error(&entry.path(), &e))?;
            }
        }
    }

    // Deepest first: a directory cannot be removed until it is empty.
    for dir in directories.into_iter().rev() {
        fs::remove_dir(&dir).map_err(|e| io_error(&dir, &e))?;
    }
    Ok(())
}

fn io_error(path: &Path, error: &std::io::Error) -> Error {
    Error::new(
        code_for(error.kind()),
        format!("{}: {error}", path.display()),
    )
}

#[cfg(unix)]
fn errno_error(path: &Path, error: rustix::io::Errno) -> Error {
    io_error(path, &std::io::Error::from(error))
}

fn code_for(kind: std::io::ErrorKind) -> ErrorCode {
    match kind {
        std::io::ErrorKind::NotFound => ErrorCode::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
        std::io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
        _ => ErrorCode::Io,
    }
}
