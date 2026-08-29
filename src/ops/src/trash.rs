//! Moving entries to the trash.
//!
//! `AGENTS.md` §8 says use native platform behaviour where users expect it,
//! and the trash is the clearest case: on macOS that is
//! `NSFileManager.trashItem`, which records what Finder needs for Put Back
//! and which knows that an item on another volume belongs in that volume's
//! `.Trashes` rather than in the home directory's.
//!
//! That call lives in the platform adapter, above this crate, so it is
//! installed here as a hook rather than called directly — this crate must not
//! contain platform SDK code (`AGENTS.md` §5, enforced by
//! `tests/tests/architecture.rs`). [`set_native_trash`] is how the adapter
//! offers it.
//!
//! Without a hook, the fallback moves the entry into the platform's trash
//! directory. That is where the trash is, but it writes no Put Back metadata
//! and cannot handle another volume properly. The fallback is not papered
//! over: it is still preferable to `delete` because the file remains, and it
//! is what runs anywhere the adapter has nothing to offer.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jtf_core::{Error, ErrorCode};

/// A platform's own "move to trash", returning where the item went.
///
/// Returning `None` means the platform declined, and the fallback runs.
pub type NativeTrash = fn(&Path) -> Option<PathBuf>;

static NATIVE_TRASH: OnceLock<NativeTrash> = OnceLock::new();

/// Install the platform's trash implementation.
///
/// Called once, early, by the platform adapter. Later calls are ignored
/// rather than replacing it: which implementation trashes a file is not
/// something that should change while the program runs.
pub fn set_native_trash(trash: NativeTrash) {
    let _ = NATIVE_TRASH.set(trash);
}

/// Whether a platform implementation is installed.
pub fn has_native_trash() -> bool {
    NATIVE_TRASH.get().is_some()
}

/// The directory the platform uses for the trash, if there is one.
///
/// macOS: `~/.Trash`. Linux: `$XDG_DATA_HOME/Trash/files`, per the
/// freedesktop specification. Windows has no directory that can be used this
/// way — the Recycle Bin needs `IFileOperation` — so it returns `None` and the
/// UI must offer permanent delete with a clear warning instead.
pub fn trash_directory() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;

    let macos = home.join(".Trash");
    if macos.is_dir() {
        return Some(macos);
    }

    let xdg = std::env::var_os("XDG_DATA_HOME")
        .map_or_else(|| home.join(".local/share"), PathBuf::from)
        .join("Trash/files");
    if xdg.is_dir() {
        return Some(xdg);
    }
    None
}

/// Move one entry to the trash.
///
/// # Errors
///
/// [`ErrorCode::Unsupported`] where there is no usable trash directory, and
/// whatever the filesystem reports otherwise.
pub(crate) fn trash_entry(source: &Path) -> Result<PathBuf, Error> {
    // The platform first. It is the only one that can record Put Back, and
    // the only one that knows which volume's trash an item belongs in.
    if let Some(native) = NATIVE_TRASH.get() {
        if let Some(destination) = native(source) {
            return Ok(destination);
        }
    }

    let Some(directory) = trash_directory() else {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "no trash directory on this platform; use permanent delete",
        ));
    };
    let Some(name) = source.file_name() else {
        return Err(Error::new(ErrorCode::InvalidPath, "entry has no name"));
    };

    // The trash holds files from everywhere, so collisions are normal rather
    // than exceptional; a trashed file must never silently replace another
    // trashed file.
    let target = crate::conflict::unique_destination(&directory.join(name))
        .ok_or_else(|| Error::new(ErrorCode::AlreadyExists, "no free name in the trash"))?;

    match fs::rename(source, &target) {
        Ok(()) => Ok(target),
        Err(error) if error.raw_os_error() == Some(18) => {
            // EXDEV: the trash is on another volume. Copy then remove, which
            // is what a cross-volume move is anywhere.
            crate::run::copy_tree(source, &target, &jtf_jobs::CancellationToken::never())?;
            crate::run::remove_tree(source)?;
            Ok(target)
        }
        Err(error) => Err(Error::new(
            ErrorCode::Io,
            format!("{} -> {}: {error}", source.display(), target.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trash_directory_is_found_on_this_platform_or_reported_as_absent() {
        // Not an assertion about which platform this is: the point is that the
        // answer is a definite Some or None, never a guess.
        let found = trash_directory();
        if let Some(directory) = found {
            assert!(
                directory.is_dir(),
                "reported a trash directory that is not one"
            );
        }
    }
}
