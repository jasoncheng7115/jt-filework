//! Creating a symbolic link.
//!
//! One function, in a crate of its own, because it is the only thing in the
//! file operations that cannot be written portably: Unix has
//! `std::os::unix::fs::symlink`, and Windows has two different calls depending
//! on whether the target is a directory, both of which need a privilege that
//! an ordinary account usually does not hold.
//!
//! It lived in `src/ops` behind a `#[cfg(unix)]` and an allowlist entry
//! promising to move it here (`AGENTS.md` §5: platform code lives in the
//! platform layer). This is that move. Nothing else in `src/ops` now knows
//! which operating system it is on.

use std::path::Path;

use jtf_core::{Error, ErrorCode};

/// Create a symbolic link at `at` pointing to `target`.
///
/// `target` is used exactly as given - a link's target is a string the
/// filesystem never resolves at creation time, and rewriting it would change
/// what the user copied.
///
/// # Errors
///
/// [`ErrorCode::Unsupported`] where the platform cannot create one, and
/// whatever the system reported otherwise.
#[cfg(unix)]
pub fn create(target: &Path, at: &Path) -> Result<(), Error> {
    std::os::unix::fs::symlink(target, at).map_err(|e| {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
            _ => ErrorCode::Io,
        };
        Error::new(code, format!("{}: {e}", at.display()))
    })
}

/// Windows needs `SeCreateSymbolicLinkPrivilege`, or Developer Mode, and needs
/// to know in advance whether the target is a directory. Copying the target's
/// contents instead would silently change what the user asked for - a link and
/// the thing it points at are not the same object, and a file manager that
/// quietly substitutes one for the other is lying about what it did.
///
/// So this reports rather than guesses, and the caller surfaces the refusal.
#[cfg(not(unix))]
pub fn create(target: &Path, at: &Path) -> Result<(), Error> {
    let _ = (target, at);
    Err(Error::new(
        ErrorCode::Unsupported,
        "copying a symbolic link is not supported on this platform yet",
    ))
}

/// Whether this build can create one at all.
///
/// So a caller can decide before it starts rather than half-way through a
/// tree: a copy that fails on the ninth of ten thousand files has already
/// written the first eight.
pub const fn supported() -> bool {
    cfg!(unix)
}
