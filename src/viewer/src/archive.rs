//! Listing what is inside a ZIP archive.
//!
//! Listing only: the central directory is read, nothing is decompressed, and
//! nothing is written. That is the whole of what CView's archive view does on
//! entry (`CV.HLP` §四: press Enter on a ZIP and see its contents), and it is
//! the part that can be done without trusting a decompressor.
//!
//! An archive is untrusted input in the strongest sense — it is a file format
//! designed to be passed around — so this parser assumes every field is
//! hostile:
//!
//! * Every offset and length is bounds-checked against the file's real size
//!   before it is used.
//! * The entry count is taken from the header but the loop is bounded by the
//!   bytes actually present, so a header claiming four billion entries reads
//!   what is there and stops.
//! * Entry names are length-limited, and a name that escapes the archive
//!   (`../`, a leading `/`, a Windows drive) is reported as unsafe rather than
//!   silently normalized. Nothing here writes files, but the flag has to
//!   travel with the entry so extraction can never be written without it.
//! * No recursion: a nested archive is an entry, not a descent.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use jtf_core::{Error, ErrorCode};

/// The largest central directory this will read.
///
/// Generous for real archives and small enough that a malformed header cannot
/// make us allocate arbitrarily (`docs/SECURITY.md` §13).
pub const MAX_DIRECTORY_BYTES: u64 = 64 * 1024 * 1024;

/// The most entries listed from one archive.
pub const MAX_ENTRIES: usize = 100_000;

/// The longest entry name kept.
pub const MAX_NAME_BYTES: usize = 4096;

/// How far back the end-of-central-directory record is searched for.
///
/// The record is at the end, followed by a comment of at most 65535 bytes.
const MAX_TRAILER_SCAN: u64 = 66 * 1024;

const END_OF_CENTRAL_DIRECTORY: [u8; 4] = [b'P', b'K', 0x05, 0x06];
const CENTRAL_FILE_HEADER: [u8; 4] = [b'P', b'K', 0x01, 0x02];

/// One entry in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// The name as stored, lossily decoded. Never used as a path directly.
    pub name: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Compressed size in bytes.
    pub compressed_size: u64,
    /// Whether the stored name is a directory entry.
    pub is_directory: bool,
    /// Whether the stored name would escape the extraction directory.
    ///
    /// Carried rather than fixed: extraction has to refuse these, and a name
    /// that was quietly rewritten is one nobody can refuse later.
    pub unsafe_name: bool,
}

/// Every entry in the ZIP at `path`.
///
/// # Errors
///
/// [`ErrorCode::ParseFailed`] when the file is not a ZIP or its directory is
/// unreadable; [`ErrorCode::Io`] when the file cannot be read.
pub fn list(path: &Path) -> Result<Vec<ArchiveEntry>, Error> {
    let mut file = File::open(path).map_err(|e| Error::new(ErrorCode::Io, format!("open: {e}")))?;
    let total = file
        .metadata()
        .map_err(|e| Error::new(ErrorCode::Io, format!("stat: {e}")))?
        .len();

    let (directory_offset, directory_size, claimed_entries) = read_trailer(&mut file, total)?;

    if directory_size > MAX_DIRECTORY_BYTES
        || directory_offset > total
        || directory_offset.saturating_add(directory_size) > total
    {
        return Err(Error::new(
            ErrorCode::ParseFailed,
            "central directory does not fit inside the file",
        ));
    }

    file.seek(SeekFrom::Start(directory_offset))
        .map_err(|e| Error::new(ErrorCode::Io, format!("seek: {e}")))?;
    let mut directory = vec![0_u8; usize::try_from(directory_size).unwrap_or(0)];
    file.read_exact(&mut directory)
        .map_err(|e| Error::new(ErrorCode::Io, format!("read: {e}")))?;

    // The header's count is a hint for reserving, never a loop bound: the
    // bytes present decide how many entries there are.
    let expected = usize::try_from(claimed_entries)
        .unwrap_or(0)
        .min(MAX_ENTRIES);
    let mut entries = Vec::with_capacity(expected);

    let mut at = 0_usize;
    while at + 46 <= directory.len() && entries.len() < MAX_ENTRIES {
        if directory[at..at + 4] != CENTRAL_FILE_HEADER {
            break;
        }
        let compressed =
            u32::from(u16_at(&directory, at + 20)) | (u32::from(u16_at(&directory, at + 22)) << 16);
        let uncompressed =
            u32::from(u16_at(&directory, at + 24)) | (u32::from(u16_at(&directory, at + 26)) << 16);
        let name_len = usize::from(u16_at(&directory, at + 28));
        let extra_len = usize::from(u16_at(&directory, at + 30));
        let comment_len = usize::from(u16_at(&directory, at + 32));

        let name_start = at + 46;
        let Some(name_end) = name_start.checked_add(name_len) else {
            break;
        };
        if name_end > directory.len() {
            break;
        }

        let raw = &directory[name_start..name_end.min(name_start + MAX_NAME_BYTES)];
        let name = String::from_utf8_lossy(raw).into_owned();
        entries.push(ArchiveEntry {
            is_directory: name.ends_with('/'),
            unsafe_name: escapes(&name),
            name,
            size: u64::from(uncompressed),
            compressed_size: u64::from(compressed),
        });

        let Some(next) = name_end
            .checked_add(extra_len)
            .and_then(|n| n.checked_add(comment_len))
        else {
            break;
        };
        if next <= at {
            break; // no forward progress: refuse to loop
        }
        at = next;
    }

    Ok(entries)
}

/// Whether a stored name would place a file outside the extraction directory.
///
/// Checked on the stored form, before any normalization, because that is the
/// form an attacker controls. Both separators are considered: a ZIP written on
/// Windows uses backslashes, and a reader that only looks for `/` is exactly
/// the reader the archive was built for.
fn escapes(name: &str) -> bool {
    if name.starts_with('/') || name.starts_with('\\') {
        return true;
    }
    // A drive letter or a UNC path.
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return true;
    }
    name.split(['/', '\\']).any(|part| part == "..")
}

/// `(offset, size, entry count)` of the central directory.
fn read_trailer(file: &mut File, total: u64) -> Result<(u64, u64, u32), Error> {
    let scan = MAX_TRAILER_SCAN.min(total);
    let start = total - scan;
    file.seek(SeekFrom::Start(start))
        .map_err(|e| Error::new(ErrorCode::Io, format!("seek: {e}")))?;
    let mut tail = vec![0_u8; usize::try_from(scan).unwrap_or(0)];
    file.read_exact(&mut tail)
        .map_err(|e| Error::new(ErrorCode::Io, format!("read: {e}")))?;

    // Searched from the end: a file containing the signature in its data must
    // not shadow the real record.
    let mut at = tail.len().saturating_sub(22);
    loop {
        if tail[at..].len() >= 22 && tail[at..at + 4] == END_OF_CENTRAL_DIRECTORY {
            let entries = u16_at(&tail, at + 10);
            let size =
                u32::from(u16_at(&tail, at + 12)) | (u32::from(u16_at(&tail, at + 14)) << 16);
            let offset =
                u32::from(u16_at(&tail, at + 16)) | (u32::from(u16_at(&tail, at + 18)) << 16);
            return Ok((u64::from(offset), u64::from(size), u32::from(entries)));
        }
        if at == 0 {
            return Err(Error::new(
                ErrorCode::ParseFailed,
                "no end-of-central-directory record; not a ZIP",
            ));
        }
        at -= 1;
    }
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    match (bytes.get(at), bytes.get(at + 1)) {
        (Some(&low), Some(&high)) => u16::from(low) | (u16::from(high) << 8),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_climbs_out_is_flagged() {
        assert!(escapes("../evil"));
        assert!(escapes("a/../../evil"));
        assert!(escapes("/absolute"));
        assert!(escapes("C:/windows"));
        assert!(
            escapes("a\\..\\..\\evil"),
            "a ZIP written on Windows separates with backslashes, and a \
             reader that only looks for forward slashes is the one the \
             archive was crafted for"
        );
    }

    #[test]
    fn ordinary_names_are_not_flagged() {
        for name in ["a.txt", "dir/b.txt", "dir/sub/c", "..hidden", "a..b"] {
            assert!(!escapes(name), "{name} does not escape");
        }
    }

    #[test]
    fn a_file_that_is_not_a_zip_is_an_error_not_a_panic() {
        let path = std::env::temp_dir().join("jtf-not-a-zip.bin");
        std::fs::write(&path, vec![0_u8; 1024]).expect("write");
        assert!(list(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_empty_file_is_an_error_not_a_panic() {
        let path = std::env::temp_dir().join("jtf-empty.zip");
        std::fs::write(&path, b"").expect("write");
        assert!(list(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_truncated_trailer_is_an_error_not_a_panic() {
        // The signature, then nothing where the record's fields should be.
        let path = std::env::temp_dir().join("jtf-truncated.zip");
        std::fs::write(&path, b"PK\x05\x06").expect("write");
        assert!(list(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod real_zip_tests {
    use super::*;

    /// Built by the platform's own `zip`, so the parser is checked against a
    /// file this project did not write. A parser tested only against its own
    /// output agrees with itself and nothing else.
    fn make_zip(name: &str) -> Option<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("jtf-zip-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).ok()?;
        std::fs::write(dir.join("one.txt"), b"hello world").ok()?;
        std::fs::write(dir.join("sub/two.txt"), vec![b'x'; 5000]).ok()?;

        let archive = std::env::temp_dir().join(format!("jtf-zip-{name}.zip"));
        let _ = std::fs::remove_file(&archive);
        let ok = std::process::Command::new("zip")
            .arg("-r")
            .arg(&archive)
            .arg(".")
            .current_dir(&dir)
            .output()
            .is_ok_and(|out| out.status.success());
        let _ = std::fs::remove_dir_all(&dir);
        ok.then_some(archive)
    }

    #[test]
    fn it_lists_a_zip_the_system_wrote() {
        let Some(archive) = make_zip("list") else {
            return; // no `zip` on this machine; nothing to check against
        };
        let entries = list(&archive).expect("a real zip lists");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(
            names.iter().any(|n| n.ends_with("one.txt")),
            "expected one.txt among {names:?}"
        );
        assert!(
            names.iter().any(|n| n.ends_with("two.txt")),
            "expected sub/two.txt among {names:?}"
        );
        let two = entries
            .iter()
            .find(|e| e.name.ends_with("two.txt"))
            .expect("two.txt");
        assert_eq!(two.size, 5000, "the uncompressed size is the real one");
        assert!(
            two.compressed_size < two.size,
            "5000 identical bytes must compress"
        );
        assert!(entries.iter().all(|e| !e.unsafe_name));
        let _ = std::fs::remove_file(&archive);
    }
}
