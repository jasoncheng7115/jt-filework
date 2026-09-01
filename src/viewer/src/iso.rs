//! Listing what is inside an ISO 9660 image (ADR-0005).
//!
//! Listing only: volume descriptors and directory records are read, nothing is
//! decompressed — an ISO stores files uncompressed — and nothing is written.
//! Pressing Enter on a `.iso` shows its contents, the same way pressing Enter
//! on a `.zip` does (`CV.HLP` §四), and this is the part that can be done
//! without trusting anything.
//!
//! An image is untrusted input in the strongest sense: it is a file format
//! designed to be downloaded. Every field here is assumed hostile:
//!
//! * Every extent and length is checked against the file's real size before it
//!   is used to read.
//! * Nothing is allocated to a size taken from the file. A directory's extent
//!   is capped at [`MAX_DIRECTORY_BYTES`] before a buffer is made for it.
//! * The walk is a queue, not recursion (`AGENTS.md` §20.2), bounded by
//!   [`MAX_DEPTH`] and [`MAX_ENTRIES`], and it remembers which extents it has
//!   already read — a directory that points at its own ancestor is a cycle,
//!   and a cycle must end.
//! * A name that would escape the extraction directory (`../`, a leading `/`,
//!   a Windows drive) is *reported* as unsafe, never rewritten, so extraction
//!   can refuse it. This is the same rule and the same flag the ZIP listing
//!   sets.
//!
//! What is understood: ISO 9660 with Joliet (UCS-2 names, preferred when the
//! image has them) and the Rock Ridge `NM` entry (POSIX names). What is not:
//! UDF. A UDF-only image is reported as unreadable rather than listed as
//! empty, because "this is empty" and "I cannot read this" are different
//! answers and only one of them is true.

use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use jtf_core::{Error, ErrorCode};

use crate::archive::{ArchiveEntry, MAX_NAME_BYTES};

/// The size of a logical sector. Fixed by the standard for the descriptors.
const SECTOR: u64 = 2048;

/// Where the volume descriptors begin. Sectors 0–15 are the system area.
const FIRST_DESCRIPTOR_SECTOR: u64 = 16;

/// How many volume descriptors will be read before giving up.
///
/// A real image has a handful. The set is supposed to end with a terminator;
/// this is what stops a file that never terminates it.
const MAX_DESCRIPTORS: u64 = 64;

/// The most entries listed from one image.
pub const MAX_ENTRIES: usize = 100_000;

/// How deep the directory walk will go.
const MAX_DEPTH: usize = 32;

/// The largest single directory extent that will be read.
///
/// A directory of 16 MiB of records is already far past anything real, and
/// this is what stops a length field from sizing an allocation.
pub const MAX_DIRECTORY_BYTES: u64 = 16 * 1024 * 1024;

/// Where a file's bytes live inside the image.
///
/// Carried alongside the listing so extraction does not have to parse the
/// image a second time to find out where anything is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    /// Byte offset of the first byte, already multiplied out from the sector.
    pub offset: u64,
    /// Length in bytes.
    pub length: u64,
}

/// One entry in an image, with where to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoEntry {
    /// The entry as the archive window and the extraction path see it.
    pub entry: ArchiveEntry,
    /// Where its bytes are. A directory's extent is where its records are and
    /// is not meant to be extracted.
    pub extent: Extent,
}

/// Every entry in the ISO image at `path`.
///
/// # Errors
///
/// [`ErrorCode::ParseFailed`] when the file is not an ISO 9660 image or its
/// structure does not fit inside it; [`ErrorCode::Io`] when it cannot be read.
pub fn list(path: &Path) -> Result<Vec<ArchiveEntry>, Error> {
    Ok(read(path)?.into_iter().map(|found| found.entry).collect())
}

/// Every entry in the image, with the location of each one's bytes.
///
/// # Errors
///
/// As [`list`].
pub fn read(path: &Path) -> Result<Vec<IsoEntry>, Error> {
    let mut file = File::open(path).map_err(|e| io_error("open", &e))?;
    let total = file.metadata().map_err(|e| io_error("stat", &e))?.len();
    if total < (FIRST_DESCRIPTOR_SECTOR + 1) * SECTOR {
        return Err(parse_error("too small to be an ISO 9660 image"));
    }

    let root = read_root_record(&mut file, total)?;
    walk(&mut file, total, root)
}

/// Whether `path` looks like something this module can read.
///
/// By extension, like the ZIP check: opening every file to look for `CD001`
/// would read from every row the cursor passes over.
#[must_use]
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("iso"))
}

/// The root directory record, and whether its tree is Joliet-encoded.
struct Root {
    record: DirectoryRecord,
    joliet: bool,
}

/// Find the primary descriptor, and prefer a Joliet one if the image has it.
///
/// Joliet is preferred because it holds the names whoever built the image
/// meant people to see; the primary tree of a Windows disc is `SETUP.EXE;1`.
fn read_root_record(file: &mut File, total: u64) -> Result<Root, Error> {
    let mut primary: Option<DirectoryRecord> = None;
    let mut joliet: Option<DirectoryRecord> = None;
    let mut sector = FIRST_DESCRIPTOR_SECTOR;
    let mut seen = 0_u64;

    while seen < MAX_DESCRIPTORS {
        let at = sector * SECTOR;
        if at.saturating_add(SECTOR) > total {
            break;
        }
        let block = read_at(file, at, SECTOR)?;
        // `CD001` at offset 1 is what makes this a volume descriptor at all.
        if &block[1..6] != b"CD001" {
            break;
        }
        match block[0] {
            // Primary volume descriptor. Its root record is 34 bytes at 156.
            1 => {
                if primary.is_none() {
                    primary = parse_record(&block[156..190], false).map(|(record, _)| record);
                }
            }
            // Supplementary. Joliet announces itself with one of three escape
            // sequences in the 32 bytes at offset 88.
            2 => {
                let escapes = &block[88..120];
                let is_joliet = escapes
                    .windows(3)
                    .any(|w| w == b"%/@" || w == b"%/C" || w == b"%/E");
                if is_joliet && joliet.is_none() {
                    joliet = parse_record(&block[156..190], true).map(|(record, _)| record);
                }
            }
            // Terminator.
            255 => break,
            _ => {}
        }
        sector += 1;
        seen += 1;
    }

    if let Some(record) = joliet {
        return Ok(Root {
            record,
            joliet: true,
        });
    }
    primary.map_or_else(
        || Err(parse_error("no readable volume descriptor")),
        |record| {
            Ok(Root {
                record,
                joliet: false,
            })
        },
    )
}

/// One directory record: where its data is, how long, and what it is called.
#[derive(Debug, Clone)]
struct DirectoryRecord {
    extent: u64,
    length: u64,
    is_directory: bool,
    name: String,
    /// 0 for `.`, 1 for `..`. These are structure, not entries.
    special: Option<u8>,
}

/// Parse one record out of `bytes`, returning it and how many bytes it used.
///
/// `None` when the record is malformed or is the zero byte that pads out the
/// end of a sector.
fn parse_record(bytes: &[u8], joliet: bool) -> Option<(DirectoryRecord, usize)> {
    let record_length = *bytes.first()? as usize;
    // A zero length is the padding at the end of a sector, not a record.
    if record_length < 33 || record_length > bytes.len() {
        return None;
    }
    let record = &bytes[..record_length];

    // Both-endian fields: the little-endian half is the first four bytes.
    let extent = u32::from_le_bytes(record.get(2..6)?.try_into().ok()?);
    let length = u32::from_le_bytes(record.get(10..14)?.try_into().ok()?);
    let flags = *record.get(25)?;
    let name_length = *record.get(32)? as usize;
    let name_end = 33usize.checked_add(name_length)?;
    if name_end > record.len() {
        return None;
    }
    let raw_name = &record[33..name_end];

    // `.` and `..` are stored as single bytes 0x00 and 0x01.
    let special = if name_length == 1 && (raw_name[0] == 0 || raw_name[0] == 1) {
        Some(raw_name[0])
    } else {
        None
    };

    let mut name = if special.is_some() {
        String::new()
    } else if joliet {
        decode_ucs2(raw_name)
    } else {
        decode_ascii_name(raw_name)
    };

    // Rock Ridge lives in the system-use area, after the name and its padding
    // byte. `NM` there is the real POSIX name and wins over the base one.
    let padding = usize::from(name_length.is_multiple_of(2));
    let system_use_start = name_end.saturating_add(padding);
    if special.is_none() && system_use_start < record.len() {
        if let Some(posix) = rock_ridge_name(&record[system_use_start..]) {
            name = posix;
        }
    }

    Some((
        DirectoryRecord {
            extent: u64::from(extent),
            length: u64::from(length),
            is_directory: flags & 0x02 != 0,
            name,
            special,
        },
        record_length,
    ))
}

/// The `NM` entry from a Rock Ridge system-use area, if there is one.
///
/// Entries are `[signature: 2][length: 1][version: 1][payload…]`. `NM`'s
/// payload is a flags byte then the name; a name can be continued across
/// several entries, which is why they are concatenated rather than taken one
/// at a time.
fn rock_ridge_name(area: &[u8]) -> Option<String> {
    let mut at = 0usize;
    let mut name = String::new();
    // Bounded by the area itself, and every step must advance.
    while at + 4 <= area.len() {
        let length = area[at + 2] as usize;
        if length < 4 || at + length > area.len() {
            break;
        }
        if &area[at..at + 2] == b"NM" {
            let payload = &area[at + 5..at + length];
            // Flags bit 0 means "continues in the next NM"; either way the
            // bytes here are part of the name.
            name.push_str(&String::from_utf8_lossy(payload));
            if name.len() > MAX_NAME_BYTES {
                name.truncate(MAX_NAME_BYTES);
                break;
            }
        }
        at += length;
    }
    (!name.is_empty()).then_some(name)
}

/// A base ISO 9660 name: uppercase, with a `;1` version suffix to drop.
fn decode_ascii_name(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let without_version = text.split(';').next().unwrap_or(&text);
    // `README.` with nothing after the dot is how a name with no extension is
    // stored; the trailing dot is padding, not part of the name.
    without_version
        .strip_suffix('.')
        .unwrap_or(without_version)
        .to_string()
}

/// A Joliet name: UCS-2, big-endian.
fn decode_ucs2(raw: &[u8]) -> String {
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    let text = String::from_utf16_lossy(&units);
    let without_version = text.split(';').next().unwrap_or(&text);
    without_version.to_string()
}

/// Walk the tree from the root, breadth first.
fn walk(file: &mut File, total: u64, root: Root) -> Result<Vec<IsoEntry>, Error> {
    let mut entries: Vec<IsoEntry> = Vec::new();
    // Extents already read. A record that points at an ancestor is a cycle,
    // and without this the walk produces entries until it hits the entry cap
    // rather than because the image ended.
    let mut visited: BTreeSet<u64> = BTreeSet::new();
    let mut queue: VecDeque<(String, DirectoryRecord, usize)> = VecDeque::new();
    queue.push_back((String::new(), root.record, 0));

    while let Some((prefix, directory, depth)) = queue.pop_front() {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        if !visited.insert(directory.extent) {
            continue; // already read: a cycle, or a directory linked twice
        }
        let at = directory.extent.saturating_mul(SECTOR);
        let length = directory.length.min(MAX_DIRECTORY_BYTES);
        if length == 0 || at.saturating_add(length) > total {
            continue; // points outside the file: skip this directory, not the image
        }
        let block = read_at(file, at, length)?;

        let mut offset = 0usize;
        while offset < block.len() {
            let Some((record, used)) = parse_record(&block[offset..], root.joliet) else {
                // A zero byte pads a record out to the end of its sector; the
                // next record starts at the next sector boundary.
                let sector = usize::try_from(SECTOR).unwrap_or(usize::MAX);
                let next = (offset / sector + 1).saturating_mul(sector);
                if next <= offset {
                    break; // cannot advance: stop rather than spin
                }
                offset = next;
                continue;
            };
            offset += used;

            // `.` and `..` are structure. Following `..` is the cycle the
            // visited set also guards, but not producing it as an entry is
            // what keeps the listing honest.
            if record.special.is_some() || record.name.is_empty() {
                continue;
            }
            if entries.len() >= MAX_ENTRIES {
                break;
            }

            let relative = if prefix.is_empty() {
                record.name.clone()
            } else {
                format!("{prefix}/{}", record.name)
            };
            let extent_at = record.extent.saturating_mul(SECTOR);
            // A file whose bytes are not inside the image is listed - it is
            // really there, in the table - but with nothing to read.
            let readable = extent_at.saturating_add(record.length) <= total;
            entries.push(IsoEntry {
                entry: ArchiveEntry {
                    name: if record.is_directory {
                        format!("{relative}/")
                    } else {
                        relative.clone()
                    },
                    size: if record.is_directory {
                        0
                    } else {
                        record.length
                    },
                    compressed_size: if record.is_directory {
                        0
                    } else {
                        record.length
                    },
                    is_directory: record.is_directory,
                    unsafe_name: escapes(&relative),
                },
                extent: Extent {
                    offset: extent_at,
                    length: if readable { record.length } else { 0 },
                },
            });

            if record.is_directory && depth + 1 < MAX_DEPTH {
                queue.push_back((relative, record, depth + 1));
            }
        }
    }

    entries.sort_by(|a, b| a.entry.name.cmp(&b.entry.name));
    Ok(entries)
}

/// Whether a stored name would land outside the folder it is extracted into.
///
/// The same rule the ZIP listing applies, spelled the same way: reported, not
/// repaired, so extraction refuses it rather than writing a rewritten name
/// nobody asked for.
fn escapes(name: &str) -> bool {
    let normalised = name.replace('\\', "/");
    normalised.starts_with('/')
        || normalised.split('/').any(|part| part == "..")
        // `C:` or a UNC prefix.
        || normalised
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':')
}

/// Read `length` bytes at `at`, having already checked they are in the file.
fn read_at(file: &mut File, at: u64, length: u64) -> Result<Vec<u8>, Error> {
    file.seek(SeekFrom::Start(at))
        .map_err(|e| io_error("seek", &e))?;
    let size = usize::try_from(length).map_err(|_| parse_error("length does not fit in memory"))?;
    let mut buffer = vec![0_u8; size];
    file.read_exact(&mut buffer)
        .map_err(|e| io_error("read", &e))?;
    Ok(buffer)
}

fn io_error(what: &str, error: &std::io::Error) -> Error {
    Error::new(ErrorCode::Io, format!("{what}: {error}"))
}

fn parse_error(what: &str) -> Error {
    Error::new(ErrorCode::ParseFailed, what.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_suffix_is_not_part_of_the_name() {
        assert_eq!(decode_ascii_name(b"SETUP.EXE;1"), "SETUP.EXE");
        assert_eq!(decode_ascii_name(b"README.;1"), "README");
        assert_eq!(decode_ascii_name(b"PLAIN"), "PLAIN");
    }

    #[test]
    fn joliet_names_are_big_endian_ucs2() {
        // "AB" as UCS-2 BE.
        assert_eq!(decode_ucs2(&[0, b'A', 0, b'B']), "AB");
    }

    #[test]
    fn a_rock_ridge_name_is_read_out_of_the_system_use_area() {
        // NM, length 9, version 1, flags 0, "real"
        let area = [b'N', b'M', 9, 1, 0, b'r', b'e', b'a', b'l'];
        assert_eq!(rock_ridge_name(&area).as_deref(), Some("real"));
    }

    /// A system-use entry claiming a length of zero must not spin the loop.
    #[test]
    fn a_zero_length_system_use_entry_terminates() {
        let area = [b'N', b'M', 0, 1, 0, b'x'];
        assert_eq!(rock_ridge_name(&area), None);
    }

    #[test]
    fn names_that_escape_are_recognised_in_every_spelling() {
        assert!(escapes("../secret"));
        assert!(escapes("a/../../secret"));
        assert!(escapes("/etc/passwd"));
        assert!(escapes(r"C:\Windows"));
        assert!(escapes(r"..\secret"));
        assert!(!escapes("normal/file.txt"));
        assert!(!escapes("a..b/file"));
    }

    #[test]
    fn only_iso_extensions_are_offered() {
        assert!(is_image(Path::new("disc.iso")));
        assert!(is_image(Path::new("DISC.ISO")));
        assert!(!is_image(Path::new("disc.img")));
        assert!(!is_image(Path::new("iso")));
    }
}
