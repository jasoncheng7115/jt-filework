//! tar, and the streams it usually arrives compressed in (ADR-0006).
//!
//! `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2` and `.tar.xz` are one format wrapped
//! in three different compressors, so they are one reader with the
//! decompressor chosen from what the file starts with. Bare `.gz`, `.bz2` and
//! `.xz` hold a single file rather than an archive, and are handled as the
//! degenerate case of the same thing.
//!
//! **Read for all four; written only as gzip.** The pure-Rust bzip2 and xz
//! crates decompress and barely compress, and adding a C library to write a
//! format nobody asks to write would be a bad trade (ADR-0006).
//!
//! An archive is untrusted input, so:
//!
//! * Every member's destination is resolved through the same
//!   [`crate::archive::safe_destination`] the ZIP and ISO paths use — one
//!   function, so a traversal cannot be refused in one format and accepted in
//!   another.
//! * A header's claimed size is never believed. What bounds the write is the
//!   count of bytes that actually came out of the decompressor, because a
//!   lying header is precisely what a decompression bomb is.
//! * Everything is streamed. A ten-gigabyte archive must not need ten
//!   gigabytes of memory, and the read has to be interruptible often enough
//!   that Cancel means something.
//! * Symlink and hard-link members are refused rather than created, and
//!   anything that is not a plain file or a directory is skipped.

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode, Result};
use jtf_jobs::CancellationToken;

use crate::archive::{safe_destination, Extracted, MAX_MEMBER_BYTES, MAX_TOTAL_BYTES};

/// How much is moved per read.
const CHUNK: usize = 64 * 1024;

/// How many bytes are read to decide what a file is.
const SNIFF: usize = 6;

/// The most members listed from one archive.
///
/// The same bound the ZIP and ISO listings use: a header claiming four billion
/// entries reads what is there and stops.
const MAX_ENTRIES: usize = 100_000;

/// Which compressor wraps a tar, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// No compression: a plain `.tar`.
    None,
    /// gzip — `.tar.gz`, `.tgz`, or a bare `.gz`.
    Gzip,
    /// bzip2 — `.tar.bz2`, or a bare `.bz2`. Read only.
    Bzip2,
    /// xz — `.tar.xz`, or a bare `.xz`. Read only.
    Xz,
}

impl Compression {
    /// Whether this build can write this one.
    ///
    /// Only gzip. See ADR-0006: the pure-Rust bzip2 and xz crates decompress
    /// and barely compress, and `.tar.gz` covers what anyone needs to create.
    #[must_use]
    pub const fn can_write(self) -> bool {
        matches!(self, Self::None | Self::Gzip)
    }

    /// The extension a newly created archive of this kind gets.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::None => "tar",
            Self::Gzip => "tar.gz",
            Self::Bzip2 => "tar.bz2",
            Self::Xz => "tar.xz",
        }
    }
}

/// What a file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    /// The stream compressor wrapping it.
    pub compression: Compression,
    /// Whether a tar archive is inside, rather than one plain file.
    pub is_tar: bool,
}

/// Decide what `path` is from its own bytes.
///
/// By content, not by name: a `.tgz` that is not one should fail to open
/// rather than open into an empty listing, and a compressed tar saved without
/// an extension is still a compressed tar.
///
/// Returns `None` for anything this module does not handle.
#[must_use]
pub fn kind_of(path: &Path) -> Option<Kind> {
    let mut file = File::open(path).ok()?;
    let mut head = [0_u8; SNIFF];
    let read = read_up_to(&mut file, &mut head).ok()?;
    let head = &head[..read];

    let compression = if head.starts_with(&[0x1f, 0x8b]) {
        Compression::Gzip
    } else if head.starts_with(b"BZh") {
        Compression::Bzip2
    } else if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Compression::Xz
    } else if is_tar(path) {
        // An uncompressed tar has no signature at the front: `ustar` sits 257
        // bytes in, which is why this is a separate look.
        return Some(Kind {
            compression: Compression::None,
            is_tar: true,
        });
    } else {
        return None;
    };

    // Whether a tar is inside can only be answered by decompressing enough of
    // it to look, so that is what happens - a few hundred bytes, not the file.
    Some(Kind {
        compression,
        is_tar: compressed_holds_tar(path, compression),
    })
}

/// The `ustar` magic at offset 257, which is what makes a file a tar.
fn is_tar(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 265];
    let Ok(read) = read_up_to(&mut file, &mut header) else {
        return false;
    };
    read >= 262 && &header[257..262] == b"ustar"
}

/// Whether a compressed stream has a tar inside.
///
/// Reads only the first block: enough for the header, and nowhere near enough
/// for a bomb to matter.
fn compressed_holds_tar(path: &Path, compression: Compression) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mut reader) = decompressor(file, compression) else {
        return false;
    };
    let mut header = [0_u8; 265];
    let Ok(read) = read_up_to(&mut reader, &mut header) else {
        return false;
    };
    read >= 262 && &header[257..262] == b"ustar"
}

/// Fill as much of `buffer` as the reader will give, short reads included.
fn read_up_to(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// A reader that yields the decompressed stream.
///
/// Boxed because the three decompressors are different types and the code
/// above them has no reason to care which one it got.
fn decompressor(file: File, compression: Compression) -> Result<Box<dyn Read>> {
    let buffered = BufReader::with_capacity(CHUNK, file);
    Ok(match compression {
        Compression::None => Box::new(buffered),
        Compression::Gzip => Box::new(flate2::read::MultiGzDecoder::new(buffered)),
        Compression::Bzip2 => Box::new(bzip2_rs::DecoderReader::new(buffered)),
        Compression::Xz => {
            // `lzma-rs` decodes into a buffer rather than offering a streaming
            // reader, so this is the one place a whole stream is held. Bounded
            // to the same per-member ceiling everything else is: an xz claiming
            // more than that is refused rather than allocated for.
            let mut input = BufReader::new(buffered);
            let mut out = Vec::new();
            lzma_rs::xz_decompress(&mut input, &mut out)
                .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("xz: {e}")))?;
            if out.len() as u64 > MAX_MEMBER_BYTES {
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "the xz stream expands past what this will unpack",
                ));
            }
            Box::new(io::Cursor::new(out))
        }
    })
}

/// One entry in a tar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarEntry {
    /// The stored name, never used as a path directly.
    pub name: String,
    /// Size in bytes as the header claims. Shown, never trusted for writing.
    pub size: u64,
    /// Whether it is a directory entry.
    pub is_directory: bool,
    /// Whether the stored name would escape the extraction folder.
    pub unsafe_name: bool,
}

/// Everything inside the archive at `path`.
///
/// # Errors
///
/// [`ErrorCode::ParseFailed`] when the file is not one of these formats or
/// cannot be read; [`ErrorCode::Io`] on a read failure.
pub fn list(path: &Path) -> Result<Vec<TarEntry>> {
    let kind = kind_of(path)
        .ok_or_else(|| Error::new(ErrorCode::Unsupported, "not a tar this build can read"))?;
    if !kind.is_tar {
        // A bare `.gz` holds one file, named after the archive without its
        // extension. It is still worth listing, so the window shows the one
        // thing that will come out.
        return Ok(vec![TarEntry {
            name: inner_name(path),
            size: 0, // not known without decompressing the whole stream
            is_directory: false,
            unsafe_name: false,
        }]);
    }

    let file = File::open(path).map_err(|e| io_error("open", &e))?;
    let mut archive = tar::Archive::new(decompressor(file, kind.compression)?);
    let mut entries = Vec::new();
    for entry in archive
        .entries()
        .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("not a readable tar: {e}")))?
    {
        let Ok(entry) = entry else {
            // A malformed member ends the listing rather than the program:
            // what was read so far is real and worth showing.
            break;
        };
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let is_directory = entry.header().entry_type().is_dir();
        entries.push(TarEntry {
            unsafe_name: escapes(&name),
            size: entry.header().size().unwrap_or(0),
            is_directory,
            name,
        });
        if entries.len() >= MAX_ENTRIES {
            break;
        }
    }
    Ok(entries)
}

/// What a bare `.gz`/`.bz2`/`.xz` unwraps to: its own name, less the suffix.
fn inner_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for suffix in [".gz", ".bz2", ".xz", ".tgz"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    format!("{name}.out")
}

/// Whether a stored name would land outside the folder it is extracted into.
fn escapes(name: &str) -> bool {
    let normalised = name.replace('\\', "/");
    normalised.starts_with('/')
        || normalised.split('/').any(|part| part == "..")
        || normalised
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':')
}

/// Extract the named members, or everything when `wanted` is empty.
///
/// # Errors
///
/// [`ErrorCode::ParseFailed`] when the archive cannot be read;
/// [`ErrorCode::Io`] on any read or write failure; [`ErrorCode::Cancelled`]
/// if the token was triggered; [`ErrorCode::LimitExceeded`] if the archive
/// expands past what this will unpack.
pub fn extract_members(
    archive_path: &Path,
    destination: &Path,
    wanted: &[String],
    cancel: &CancellationToken,
    mut progress: impl FnMut(&Extracted),
) -> Result<Extracted> {
    let kind = kind_of(archive_path)
        .ok_or_else(|| Error::new(ErrorCode::Unsupported, "not a tar this build can read"))?;
    fs::create_dir_all(destination)
        .map_err(|e| Error::new(ErrorCode::Io, format!("create destination: {e}")))?;

    let file = File::open(archive_path).map_err(|e| io_error("open", &e))?;
    let stream = decompressor(file, kind.compression)?;
    let mut done = Extracted::default();

    if !kind.is_tar {
        // One compressed file, not an archive.
        let name = inner_name(archive_path);
        if escapes(&name) {
            done.refused += 1;
            return Ok(done);
        }
        let Some(target) = safe_destination(destination, &name) else {
            done.refused += 1;
            return Ok(done);
        };
        let mut reader = stream;
        copy_bounded(&mut reader, &target, cancel, &mut done)?;
        done.files += 1;
        progress(&done);
        return Ok(done);
    }

    let mut archive = tar::Archive::new(stream);
    for entry in archive
        .entries()
        .map_err(|e| Error::new(ErrorCode::ParseFailed, format!("not a readable tar: {e}")))?
    {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let Ok(mut entry) = entry else {
            break; // a malformed member ends the walk; what was written stays
        };
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        // Not asked for. Skipped before the checks below, so a member nobody
        // picked is neither written nor counted as refused.
        if !wanted.is_empty() && !wanted.contains(&name) {
            continue;
        }

        // Refused, not repaired. `safe_destination` alone would strip the
        // leading slash off `/tmp/x` and write it as `dest/tmp/x` - contained,
        // but silently renamed, which ADR-0003 forbids in as many words. The
        // same flag the listing shows is what refuses it here, so what the
        // window marks as unsafe is exactly what extraction will not write.
        if escapes(&name) {
            done.refused += 1;
            continue;
        }

        let entry_type = entry.header().entry_type();
        // A link member is refused rather than created: writing one now means
        // a later write through it lands wherever the archive chose.
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            done.refused += 1;
            continue;
        }
        let Some(target) = safe_destination(destination, &name) else {
            done.refused += 1;
            continue;
        };
        if entry_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
            done.folders += 1;
            continue;
        }
        if !entry_type.is_file() {
            // Devices, fifos and the rest are not files to copy out.
            done.refused += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
        }
        copy_bounded(&mut entry, &target, cancel, &mut done)?;
        done.files += 1;
        progress(&done);
    }
    Ok(done)
}

/// Copy a member out, counting what actually arrives.
///
/// The header's size is not consulted: a lying header is the whole point of a
/// decompression bomb, so the ceiling is checked against bytes produced.
fn copy_bounded(
    reader: &mut impl Read,
    target: &Path,
    cancel: &CancellationToken,
    done: &mut Extracted,
) -> Result<()> {
    let mut out =
        File::create(target).map_err(|e| Error::new(ErrorCode::Io, format!("create file: {e}")))?;
    let mut buffer = vec![0_u8; CHUNK];
    let mut written = 0_u64;
    loop {
        if cancel.is_cancelled() {
            // The partial file goes: a cancelled extraction should not leave
            // something that looks extracted.
            drop(out);
            let _ = fs::remove_file(target);
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::new(ErrorCode::Io, format!("read member: {e}"))),
        };
        written += read as u64;
        done.bytes += read as u64;
        if written > MAX_MEMBER_BYTES || done.bytes > MAX_TOTAL_BYTES {
            drop(out);
            let _ = fs::remove_file(target);
            return Err(Error::new(
                ErrorCode::LimitExceeded,
                "the archive expands past what this will unpack",
            ));
        }
        out.write_all(&buffer[..read])
            .map_err(|e| Error::new(ErrorCode::Io, format!("write: {e}")))?;
    }
    Ok(())
}

/// Create a `.tar` or `.tar.gz` at `archive_path` holding `sources`.
///
/// Directories are added with their contents. Symlinks are skipped rather
/// than followed, for the same reason extraction refuses them.
///
/// # Errors
///
/// [`ErrorCode::Unsupported`] for a compression this build cannot write;
/// [`ErrorCode::Io`] on any read or write failure; [`ErrorCode::Cancelled`]
/// if the token was triggered.
pub fn create(
    archive_path: &Path,
    sources: &[PathBuf],
    compression: Compression,
    cancel: &CancellationToken,
    mut progress: impl FnMut(u64),
) -> Result<u64> {
    if !compression.can_write() {
        return Err(Error::new(
            ErrorCode::Unsupported,
            "this build writes tar and tar.gz only (ADR-0006)",
        ));
    }
    let file = File::create(archive_path)
        .map_err(|e| Error::new(ErrorCode::Io, format!("create archive: {e}")))?;

    let mut count = 0_u64;
    // The two writers differ in type, so the walk is a closure used by both
    // rather than the same code written twice.
    if compression == Compression::Gzip {
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        add_all(&mut builder, sources, cancel, &mut count, &mut progress)?;
        builder
            .into_inner()
            .and_then(flate2::write::GzEncoder::finish)
            .map_err(|e| Error::new(ErrorCode::Io, format!("finish archive: {e}")))?;
    } else {
        let mut builder = tar::Builder::new(file);
        add_all(&mut builder, sources, cancel, &mut count, &mut progress)?;
        builder
            .into_inner()
            .map_err(|e| Error::new(ErrorCode::Io, format!("finish archive: {e}")))?;
    }
    Ok(count)
}

/// Add every source to `builder`, walking directories.
fn add_all<W: Write>(
    builder: &mut tar::Builder<W>,
    sources: &[PathBuf],
    cancel: &CancellationToken,
    count: &mut u64,
    progress: &mut impl FnMut(u64),
) -> Result<()> {
    for source in sources {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        let Some(name) = source.file_name() else {
            continue;
        };
        // `symlink_metadata`, so a link is seen as a link and skipped rather
        // than followed into whatever it points at.
        let Ok(meta) = fs::symlink_metadata(source) else {
            continue;
        };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            builder
                .append_dir_all(name, source)
                .map_err(|e| Error::new(ErrorCode::Io, format!("add directory: {e}")))?;
        } else {
            builder
                .append_path_with_name(source, name)
                .map_err(|e| Error::new(ErrorCode::Io, format!("add file: {e}")))?;
        }
        *count += 1;
        progress(*count);
    }
    Ok(())
}

fn io_error(what: &str, error: &std::io::Error) -> Error {
    Error::new(ErrorCode::Io, format!("{what}: {error}"))
}
