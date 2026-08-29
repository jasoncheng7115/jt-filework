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
//! directory and, where that trash is freedesktop-shaped, writes the
//! `.trashinfo` record every Linux file manager reads to offer Restore. What
//! it still cannot do is pick another volume's trash, which needs the
//! platform. The fallback is not papered over: it is what runs anywhere the
//! adapter has nothing to offer.

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

/// Record where a trashed entry came from, per the freedesktop trash
/// specification.
///
/// `Trash/files/NAME` gets a matching `Trash/info/NAME.trashinfo` naming the
/// original path and the time. Every Linux file manager reads these, so
/// writing one is what makes "Restore" work there — the same thing macOS gets
/// from `NSFileManager.trashItem`, which is why this runs only in the
/// fallback.
///
/// Best effort by design: failing to write the note must not stop the file
/// reaching the trash, because a trashed file without a note is still safer
/// than a file left where the user asked for it to be removed.
fn write_restore_record(trash_files: &Path, target: &Path, source: &Path) {
    // Only for a freedesktop-shaped trash: macOS's `~/.Trash` has no `info`
    // directory and inventing one there would litter it.
    let Some(root) = trash_files.parent() else {
        return;
    };
    let info_dir = root.join("info");
    if !info_dir.is_dir() {
        return;
    }
    let Some(name) = target.file_name().and_then(|n| n.to_str()) else {
        return;
    };

    let deleted_at = std::time::SystemTime::now();
    let record = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode(&source.to_string_lossy()),
        iso8601_local(deleted_at)
    );
    let _ = fs::write(info_dir.join(format!("{name}.trashinfo")), record);
}

/// Percent-encode a path for a `.trashinfo` file.
///
/// The specification requires it, and it is what stops a path containing a
/// newline from writing a second key into the file — a `Path=` value that can
/// inject `DeletionDate=` is a small injection in a small format, and small
/// is not the same as harmless.
fn percent_encode(path: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char);
            }
            other => {
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

/// `YYYY-MM-DDThh:mm:ss`, which is what the specification asks for.
///
/// Computed here rather than pulled in with a date library: this is the only
/// place the program formats a date for a machine to read, and a dependency
/// for one format string is a dependency to audit for one format string.
fn iso8601_local(at: std::time::SystemTime) -> String {
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    // Civil-from-days, the standard algorithm. UTC: the specification allows
    // it, and guessing a local offset without a timezone database would be
    // guessing.
    // Saturating rather than wrapping: a clock far enough in the future to
    // overflow this would otherwise write a date in the past.
    let days = i64::try_from(secs / 86_400).unwrap_or(i64::MAX);
    let time_of_day = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{year:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60
    )
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

    // The restore record is written *before* the move, so a crash between the
    // two leaves an orphaned info file rather than a trashed file nobody can
    // put back. An orphan is tidy-up; a file with no record is lost work.
    write_restore_record(&directory, &target, source);

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod restore_record_tests {
    use super::{iso8601_local, percent_encode, write_restore_record};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-trashinfo-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files")).unwrap();
        std::fs::create_dir_all(dir.join("info")).unwrap();
        dir
    }

    #[test]
    fn a_freedesktop_trash_gets_its_record() {
        let root = scratch("writes");
        let files = root.join("files");
        write_restore_record(
            &files,
            &files.join("gone.txt"),
            Path::new("/home/me/gone.txt"),
        );

        let info = root.join("info/gone.txt.trashinfo");
        assert!(
            info.is_file(),
            "this record is what makes Restore work in every Linux file manager"
        );
        let text = std::fs::read_to_string(&info).unwrap();
        assert!(text.starts_with("[Trash Info]\n"), "{text}");
        assert!(text.contains("Path=/home/me/gone.txt"), "{text}");
        assert!(text.contains("DeletionDate="), "{text}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_trash_with_no_info_directory_is_left_alone() {
        // macOS's ~/.Trash is a plain directory. Inventing an `info` folder
        // there would litter somewhere the user looks.
        let root = std::env::temp_dir().join("jtf-trashinfo-plain");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        write_restore_record(
            &root,
            &root.join("gone.txt"),
            Path::new("/home/me/gone.txt"),
        );
        let stray: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
        assert!(stray.is_empty(), "nothing was written beside the trash");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_that_could_inject_a_second_key_is_encoded() {
        let hostile = "/tmp/a\nDeletionDate=1999-01-01T00:00:00";
        let encoded = percent_encode(hostile);
        assert!(
            !encoded.contains('\n'),
            "a newline in a path must not be able to write a second key into \
             the record: {encoded}"
        );
        assert!(encoded.starts_with("/tmp/a%0A"));
    }

    #[test]
    fn ordinary_paths_stay_readable() {
        assert_eq!(
            percent_encode("/home/someone/notes-2026.txt"),
            "/home/someone/notes-2026.txt",
            "encoding everything would be correct and unreadable; the \
             specification's unreserved set is left alone"
        );
        assert_eq!(percent_encode("/tmp/a b"), "/tmp/a%20b");
        assert_eq!(percent_encode("/tmp/檔案"), "/tmp/%E6%AA%94%E6%A1%88");
    }

    #[test]
    fn the_date_is_the_shape_the_specification_asks_for() {
        // 2001-09-09T01:46:40Z, a value with a known answer.
        let at = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        assert_eq!(iso8601_local(at), "2001-09-09T01:46:40");
    }

    #[test]
    fn the_epoch_itself_formats() {
        assert_eq!(iso8601_local(UNIX_EPOCH), "1970-01-01T00:00:00");
    }

    #[test]
    fn a_leap_day_is_not_off_by_one() {
        // 2024-02-29T12:00:00Z, written as the arithmetic that produces it
        // rather than as one large number of seconds - which is both what the
        // lint asks for and clearer about where the noon comes from.
        // Duration::from_days is still unstable on this toolchain.
        let at = UNIX_EPOCH + Duration::from_secs(19_782 * 86_400 + 12 * 3_600);
        assert_eq!(iso8601_local(at), "2024-02-29T12:00:00");
    }
}
