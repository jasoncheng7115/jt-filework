//! Taking files out of an ISO 9660 image (ADR-0005).
//!
//! Read only. An image is a disc filesystem, and writing into one means
//! rewriting a structure whose only copy is the file in front of you — the
//! same reason ADR-0003 keeps "delete a member" out of the ZIP work, doubled.
//!
//! Nothing here decompresses: a file inside an image is a contiguous run of
//! sectors, so extraction is a bounded copy from an offset the listing already
//! worked out. What that removes is the zip-bomb problem — the stored size is
//! the extracted size, and the listing has already checked that the run is
//! inside the file.
//!
//! What it does not remove is the hostile *name*. A record can be called
//! `../../etc/passwd`, and it is refused here by the same
//! [`crate::archive::safe_destination`] the ZIP path uses. One function, so a
//! traversal cannot be refused in one format and accepted in the other.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use jtf_core::{Error, ErrorCode, Result};
use jtf_jobs::CancellationToken;

use crate::archive::{safe_destination, Extracted, MAX_MEMBER_BYTES, MAX_TOTAL_BYTES};

/// How much is moved per read.
const CHUNK: usize = 64 * 1024;

/// Copy everything out of the image at `image` into `destination`.
///
/// # Errors
///
/// [`ErrorCode::ParseFailed`] when the image cannot be read;
/// [`ErrorCode::Io`] on any read or write failure; [`ErrorCode::Cancelled`]
/// if the token was triggered.
pub fn extract(
    image: &Path,
    destination: &Path,
    cancel: &CancellationToken,
    progress: impl FnMut(&Extracted),
) -> Result<Extracted> {
    extract_members(image, destination, &[], cancel, progress)
}

/// Copy the named members out, or everything when `wanted` is empty.
///
/// The names are the ones the listing gave, so `C` in the archive window means
/// here what it means for a ZIP.
///
/// # Errors
///
/// As [`extract`].
pub fn extract_members(
    image: &Path,
    destination: &Path,
    wanted: &[String],
    cancel: &CancellationToken,
    mut progress: impl FnMut(&Extracted),
) -> Result<Extracted> {
    let entries = jtf_viewer::read_iso(image)?;
    let mut file =
        File::open(image).map_err(|e| Error::new(ErrorCode::Io, format!("open image: {e}")))?;

    fs::create_dir_all(destination)
        .map_err(|e| Error::new(ErrorCode::Io, format!("create destination: {e}")))?;

    let mut done = Extracted::default();
    for found in entries {
        if cancel.is_cancelled() {
            return Err(Error::bare(ErrorCode::Cancelled));
        }
        // Not asked for. Skipped before the checks below, so a member nobody
        // picked is neither written nor counted as refused - it was never a
        // candidate.
        if !wanted.is_empty() && !wanted.contains(&found.entry.name) {
            continue;
        }

        // The listing already decided this name climbs out; refusing on the
        // flag as well as on the resolution means a disagreement between the
        // two is a refusal rather than a write.
        if found.entry.unsafe_name {
            done.refused += 1;
            continue;
        }
        let Some(target) = safe_destination(destination, &found.entry.name) else {
            done.refused += 1;
            continue;
        };

        if found.entry.is_directory {
            fs::create_dir_all(&target)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
            done.folders += 1;
            continue;
        }

        if found.extent.length > MAX_MEMBER_BYTES {
            return Err(Error::new(
                ErrorCode::LimitExceeded,
                format!("a member claims {} bytes", found.extent.length),
            ));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::new(ErrorCode::Io, format!("create directory: {e}")))?;
        }
        file.seek(SeekFrom::Start(found.extent.offset))
            .map_err(|e| Error::new(ErrorCode::Io, format!("seek: {e}")))?;
        let mut out = File::create(&target)
            .map_err(|e| Error::new(ErrorCode::Io, format!("create file: {e}")))?;

        let mut remaining = found.extent.length;
        let mut buffer = vec![0_u8; CHUNK];
        while remaining > 0 {
            if cancel.is_cancelled() {
                // The partial file goes: a cancelled extraction should not
                // leave something that looks extracted.
                drop(out);
                let _ = fs::remove_file(&target);
                return Err(Error::bare(ErrorCode::Cancelled));
            }
            let want = usize::try_from(remaining.min(CHUNK as u64)).unwrap_or(CHUNK);
            let read = match file.read(&mut buffer[..want]) {
                // The listing checked the run is inside the file, so a short
                // read means the file changed underneath us. Stopping is
                // right; pretending the rest was zeroes is not.
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::new(ErrorCode::Io, format!("read image: {e}"))),
            };
            remaining -= read as u64;
            done.bytes += read as u64;
            if done.bytes > MAX_TOTAL_BYTES {
                drop(out);
                let _ = fs::remove_file(&target);
                return Err(Error::new(
                    ErrorCode::LimitExceeded,
                    "the image holds more than this will unpack",
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
