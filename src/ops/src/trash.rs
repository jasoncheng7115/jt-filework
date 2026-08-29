//! Moving entries to the trash.
//!
//! `AGENTS.md` §8 says use native platform behaviour where users expect it,
//! and the trash is the clearest case: on macOS that is
//! `NSFileManager.trashItem`, which records the information Finder needs for
//! Put Back. That call needs the platform adapter, which is Phase 4 work.
//!
//! Until then this moves the entry into the platform's trash directory, which
//! is where the trash actually is. What it does **not** do is write the
//! Put Back metadata, so a restored item has to be dragged back by hand. That
//! is a real limitation, it is documented here and in `TODO.md`, and it is not
//! papered over: `trash` is still preferable to `delete` because the file
//! remains.

use std::fs;
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};

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
