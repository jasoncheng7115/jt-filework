//! Writing a disk image to a removable disk.
//!
//! The part people call "burning", which since optical media stopped being the
//! usual target means: copy every byte of a file onto a block device, starting
//! at sector zero, destroying whatever was there.
//!
//! # What this crate is, and is not
//!
//! It is the *engine*: a byte pump with progress, cancellation, a checksum and
//! a read-back comparison. It works on any [`Read`] and any [`Write`], which
//! means the whole of it is tested against files and in-memory buffers, with no
//! disk plugged in and nothing at risk.
//!
//! It is not the part that decides which disk. That is
//! [`jtf_platform_devices`], and it is deliberately a different crate, because
//! the two failures are different: a bug here writes the wrong bytes to the
//! right disk, and a bug there writes the right bytes to the wrong one.
//!
//! # Why it reads the disk back
//!
//! A USB stick that has started to fail accepts writes and returns different
//! bytes. So does a counterfeit one, which reports a capacity it does not have
//! and silently wraps. Neither reports an error at write time. The only way to
//! find out before the disk is used to install an operating system is to read
//! it back and compare, which is what [`verify`] does and why it is on by
//! default.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};
use jtf_jobs::{CancellationToken, Progress};
use jtf_platform_devices::Device;
use serde::{Deserialize, Serialize};

mod crc;
mod run;

pub use crc::Crc32;
pub use run::{run, Silent, Stage, Watcher};

/// How much is moved per read and per write.
///
/// Four mebibytes: large enough that the per-call overhead of a raw device
/// write disappears, small enough that cancellation feels immediate and that a
/// buffer of this size is not itself a memory problem. It is a whole number of
/// sectors at every sector size in use, which the raw device on macOS requires.
pub const CHUNK: usize = 4 * 1024 * 1024;

/// The alignment a raw block device write has to satisfy.
///
/// 512 bytes even on a 4 KiB-sector disk, because the kernel presents the
/// smaller unit. An image whose length is not a multiple of this is padded with
/// zeros for the final write only; the recorded length stays the image's own.
pub const SECTOR: u64 = 512;

/// What is to be written, and where.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// The image file.
    pub image: PathBuf,
    /// Its length in bytes.
    pub image_size: u64,
    /// The disk it goes to.
    pub device: Device,
    /// Whether to read the disk back and compare afterwards.
    ///
    /// Defaults to on, and the caller should need a reason to turn it off. A
    /// write that was not checked has not been shown to have worked.
    pub verify: bool,
}

impl Plan {
    /// A plan to write `image` to `device`, with verification on.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::NotFound`] if the image is not there, [`ErrorCode::WrongKind`]
    /// if it is not a regular file, and whatever [`jtf_platform_devices::check`]
    /// refused for otherwise.
    pub fn new(image: &Path, device: Device) -> Result<Self, Error> {
        let meta = std::fs::metadata(image).map_err(|e| {
            let code = if e.kind() == std::io::ErrorKind::NotFound {
                ErrorCode::NotFound
            } else {
                ErrorCode::Io
            };
            Error::new(code, format!("{}: {e}", image.display()))
        })?;
        if !meta.is_file() {
            return Err(Error::new(
                ErrorCode::WrongKind,
                format!("{} is not a file", image.display()),
            ));
        }
        let image_size = meta.len();
        jtf_platform_devices::check(&device, image, image_size).map_err(|refusal| {
            Error::new(
                ErrorCode::PermissionDenied,
                format!("{}: {refusal:?}", device.node.display()),
            )
        })?;
        Ok(Self {
            image: image.to_path_buf(),
            image_size,
            device,
            verify: true,
        })
    }

    /// The number of bytes actually sent to the disk, image plus padding.
    pub const fn padded_size(&self) -> u64 {
        pad_to_sector(self.image_size)
    }
}

/// What happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    /// Bytes of the image written. Not the padded figure: padding is not part
    /// of the image and saying it was would overstate what was copied.
    pub written: u64,
    /// CRC-32 of those bytes, for the log and for the user to compare against
    /// a published one.
    pub checksum: u32,
    /// Bytes read back and found identical, where verification ran.
    pub verified: Option<u64>,
}

/// Round a length up to a whole number of sectors.
const fn pad_to_sector(bytes: u64) -> u64 {
    // No overflow in practice - an image within a sector of u64::MAX does not
    // exist - but saturating rather than wrapping, because wrapping here would
    // turn a huge image into a tiny write.
    bytes.saturating_add(SECTOR - 1) / SECTOR * SECTOR
}

/// Copy `total` bytes from `source` to `sink`, reporting progress.
///
/// Returns the CRC-32 of what was read from the source. The sink receives the
/// same bytes, padded with zeros to a sector boundary if `pad` is set — which
/// it must be for a raw device and must not be for a file being compared
/// byte-for-byte afterwards.
///
/// # Errors
///
/// [`ErrorCode::Cancelled`] if the token was cancelled, [`ErrorCode::Io`] for a
/// read or write failure, and [`ErrorCode::ParseFailed`] if the source ended
/// early — an image that is shorter than its own metadata said is not an image
/// this program will write half of.
pub fn copy(
    source: &mut dyn Read,
    sink: &mut dyn Write,
    total: u64,
    pad: bool,
    on_progress: &mut dyn FnMut(Progress),
    cancel: &CancellationToken,
) -> Result<u32, Error> {
    let mut buffer = vec![0_u8; CHUNK];
    let mut crc = Crc32::new();
    let mut done = 0_u64;
    on_progress(Progress::with_total(total));

    while done < total {
        cancel.check()?;
        let want = usize::try_from((total - done).min(CHUNK as u64)).unwrap_or(CHUNK);
        read_exact_or_short(source, &mut buffer[..want])?;
        crc.update(&buffer[..want]);

        let out = if pad {
            // Only the final chunk can be unaligned, and only then is anything
            // added. The zeros land past the end of the image, on a part of the
            // disk that is being overwritten anyway.
            let padded = usize::try_from(pad_to_sector(want as u64)).unwrap_or(want);
            buffer[want..padded].fill(0);
            &buffer[..padded]
        } else {
            &buffer[..want]
        };
        sink.write_all(out)
            .map_err(|e| Error::new(ErrorCode::Io, format!("writing to the disk: {e}")))?;

        done += want as u64;
        on_progress(Progress::with_total(total).set_completed(done));
    }

    // Without this the last few megabytes are still in a buffer when the
    // program says it has finished, and the user pulls the stick out.
    sink.flush()
        .map_err(|e| Error::new(ErrorCode::Io, format!("flushing the disk: {e}")))?;
    Ok(crc.finish())
}

/// Read `total` bytes from each of `source` and `written` and compare them.
///
/// With `pad` set, `written` is read a whole number of sectors at a time and
/// the padding past the image is ignored: a raw device refuses a read that is
/// not sector-sized, the same rule `copy` pads its last write for.
///
/// # Errors
///
/// [`ErrorCode::Cancelled`] if cancelled, [`ErrorCode::Io`] for a read failure,
/// and [`ErrorCode::ParseFailed`] with the byte offset if they differ — which
/// on a real disk means the disk is failing or is not the size it claims.
pub fn verify(
    source: &mut dyn Read,
    written: &mut dyn Read,
    total: u64,
    pad: bool,
    on_progress: &mut dyn FnMut(Progress),
    cancel: &CancellationToken,
) -> Result<u64, Error> {
    let mut want_buf = vec![0_u8; CHUNK];
    let mut got_buf = vec![0_u8; CHUNK];
    let mut done = 0_u64;
    on_progress(Progress::with_total(total));

    while done < total {
        cancel.check()?;
        let want = usize::try_from((total - done).min(CHUNK as u64)).unwrap_or(CHUNK);
        read_exact_or_short(source, &mut want_buf[..want])?;
        let read = if pad {
            usize::try_from(pad_to_sector(want as u64)).unwrap_or(want)
        } else {
            want
        };
        read_exact_or_short(written, &mut got_buf[..read])?;

        if want_buf[..want] != got_buf[..want] {
            let offset = done + first_difference(&want_buf[..want], &got_buf[..want]);
            return Err(Error::new(
                ErrorCode::ParseFailed,
                format!("the disk reads back differently from byte {offset}"),
            ));
        }
        done += want as u64;
        on_progress(Progress::with_total(total).set_completed(done));
    }
    Ok(done)
}

/// Which of an image's own structures says how long it should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layout {
    /// A GUID partition table, whose backup header is the image's last sector.
    Gpt,
    /// An MBR partition table, whose partitions end somewhere.
    Mbr,
    /// An ISO 9660 volume, whose primary descriptor gives its size.
    Iso9660,
}

/// The length an image's own structures say it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declared {
    /// Bytes the image needs to hold everything its table describes.
    pub needs: u64,
    /// What said so.
    pub by: Layout,
}

/// What an image says about its own length, if it says anything.
///
/// A disk image carries its own map - a partition table, or a volume
/// descriptor - and the map says how long the image has to be. A file shorter
/// than that is not the image: a download that stopped, or a decompression
/// that did. Written to a disk it gives one that does not boot, and nothing
/// at write time says so. Checked before writing, so the dialog can: a
/// Steam Deck repair image cut to 324 MB of its 8.12 GB was written to a USB
/// stick, and the only sign was that the stick would not start.
///
/// `None` for anything with no map this recognises - a raw dump, a filesystem
/// with no partition table - which is not the same as a map that fits.
///
/// # Errors
///
/// Whatever the file system reports when the image cannot be opened or read.
pub fn declared_size(image: &Path) -> Result<Option<Declared>, Error> {
    use std::io::{Seek, SeekFrom};
    let io = |e: std::io::Error| Error::new(ErrorCode::Io, format!("{}: {e}", image.display()));
    let mut file = std::fs::File::open(image).map_err(io)?;
    let mut head = [0_u8; 1024];
    let got = read_up_to(&mut file, &mut head).map_err(io)?;
    let head = &head[..got];

    // GPT: the header in sector 1 names the sector the backup header is in,
    // and that is the last sector of the disk it was made for.
    if head.len() >= 512 + 40 && &head[512..520] == b"EFI PART" {
        let backup = u64::from_le_bytes(head[512 + 32..512 + 40].try_into().unwrap_or([0; 8]));
        if backup > 1 {
            return Ok(Some(Declared {
                needs: backup.saturating_add(1).saturating_mul(SECTOR),
                by: Layout::Gpt,
            }));
        }
    }

    // MBR: where the last partition ends. Only a table whose entries look like
    // entries - boot flag 0x00 or 0x80 - because a filesystem written straight
    // onto a stick carries the same 55 AA and boot code where the table would
    // be, and reading code as a partition gives nonsense lengths.
    if head.len() >= 512 && head[510] == 0x55 && head[511] == 0xAA {
        let mut end = 0_u64;
        let mut valid = true;
        for entry in head[446..510].as_chunks::<16>().0 {
            if entry[0] != 0x00 && entry[0] != 0x80 {
                valid = false;
                break;
            }
            let kind = entry[4];
            let start = u32::from_le_bytes(entry[8..12].try_into().unwrap_or([0; 4]));
            let count = u32::from_le_bytes(entry[12..16].try_into().unwrap_or([0; 4]));
            // Empty, or the protective entry a GPT disk carries.
            if kind == 0x00 || kind == 0xEE || count == 0 {
                continue;
            }
            end = end.max((u64::from(start) + u64::from(count)) * SECTOR);
        }
        if valid && end > 0 {
            return Ok(Some(Declared {
                needs: end,
                by: Layout::Mbr,
            }));
        }
    }

    // ISO 9660: the primary volume descriptor in sector 16 of 2048 bytes gives
    // the volume's size in blocks and the block size.
    let mut descriptor = [0_u8; 2048];
    if file.seek(SeekFrom::Start(16 * 2048)).is_ok()
        && read_up_to(&mut file, &mut descriptor).map_err(io)? == descriptor.len()
        && descriptor[0] == 1
        && &descriptor[1..6] == b"CD001"
    {
        let blocks = u32::from_le_bytes(descriptor[80..84].try_into().unwrap_or([0; 4]));
        let block = u16::from_le_bytes(descriptor[128..130].try_into().unwrap_or([0; 2]));
        if blocks > 0 && block > 0 {
            return Ok(Some(Declared {
                needs: u64::from(blocks) * u64::from(block),
                by: Layout::Iso9660,
            }));
        }
    }
    Ok(None)
}

/// Read until `buffer` is full or the file ends, and say how much arrived.
fn read_up_to(file: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Where two equal-length slices first differ.
///
/// Reported to the user because the offset says which kind of failure it was:
/// a counterfeit stick diverges at the point it wraps, and a dying one
/// diverges somewhere arbitrary.
fn first_difference(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b.iter())
        .position(|(x, y)| x != y)
        .map_or(0, |i| i as u64)
}

/// Fill `buffer` completely, treating a short read as a malformed source.
fn read_exact_or_short(source: &mut dyn Read, buffer: &mut [u8]) -> Result<(), Error> {
    match source.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(Error::new(
            ErrorCode::ParseFailed,
            "the image ended before its stated length",
        )),
        Err(e) => Err(Error::new(ErrorCode::Io, format!("reading the image: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jtf_jobs::CancellationToken;
    use jtf_platform_devices::Bus;

    fn nothing(_: Progress) {}

    fn image(bytes: usize) -> Vec<u8> {
        // Not all one value: a comparison against a run of identical bytes
        // passes even when the offsets are wrong.
        (0..bytes)
            .map(|i| u8::try_from(i % 251).unwrap_or(0))
            .collect()
    }

    #[test]
    fn every_byte_arrives_and_the_checksum_covers_them() {
        let source = image(3_000);
        let mut sink = Vec::new();
        let crc = copy(
            &mut source.as_slice(),
            &mut sink,
            3_000,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(sink, source);
        assert_eq!(crc, Crc32::of(&source));
    }

    #[test]
    fn a_write_to_a_raw_device_is_padded_up_to_a_sector() {
        // 3000 bytes is 5 sectors and 440 bytes. The device gets 3072.
        let source = image(3_000);
        let mut sink = Vec::new();
        copy(
            &mut source.as_slice(),
            &mut sink,
            3_000,
            true,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(sink.len(), 3_072, "not rounded up to a whole sector");
        assert_eq!(&sink[..3_000], &source[..], "the image itself changed");
        assert!(
            sink[3_000..].iter().all(|b| *b == 0),
            "the padding is not zeros"
        );
    }

    #[test]
    fn an_image_that_is_already_a_whole_number_of_sectors_gains_nothing() {
        let source = image(4_096);
        let mut sink = Vec::new();
        copy(
            &mut source.as_slice(),
            &mut sink,
            4_096,
            true,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(sink.len(), 4_096);
    }

    #[test]
    fn an_image_larger_than_one_chunk_is_copied_across_several() {
        let source = image(CHUNK + 1_000);
        let mut sink = Vec::new();
        copy(
            &mut source.as_slice(),
            &mut sink,
            (CHUNK + 1_000) as u64,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(sink, source);
    }

    #[test]
    fn progress_never_goes_backwards_and_ends_exactly_at_the_total() {
        let source = image(CHUNK * 2 + 7);
        let total = source.len() as u64;
        let mut seen = Vec::new();
        let mut sink = Vec::new();
        copy(
            &mut source.as_slice(),
            &mut sink,
            total,
            false,
            &mut |p| seen.push(p.completed()),
            &CancellationToken::never(),
        )
        .unwrap();
        assert!(seen.windows(2).all(|w| w[1] >= w[0]), "{seen:?}");
        assert_eq!(*seen.last().unwrap(), total);
        assert!(seen.iter().all(|c| *c <= total), "progress exceeded total");
    }

    #[test]
    fn a_cancelled_write_stops_and_says_so() {
        let source = image(CHUNK * 4);
        let mut sink = Vec::new();
        let err = copy(
            &mut source.as_slice(),
            &mut sink,
            (CHUNK * 4) as u64,
            false,
            &mut nothing,
            &CancellationToken::cancelled(),
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Cancelled);
        assert!(sink.is_empty(), "wrote something after being cancelled");
    }

    #[test]
    fn an_image_shorter_than_it_claims_is_refused_rather_than_half_written() {
        // A truncated download. Writing the part that arrived produces a stick
        // that boots partway and then does something undefined.
        let source = image(100);
        let mut sink = Vec::new();
        let err = copy(
            &mut source.as_slice(),
            &mut sink,
            5_000,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::ParseFailed);
    }

    #[test]
    fn verification_passes_when_the_disk_reads_back_the_same() {
        let source = image(10_000);
        let disk = source.clone();
        let checked = verify(
            &mut source.as_slice(),
            &mut disk.as_slice(),
            10_000,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(checked, 10_000);
    }

    #[test]
    fn verification_names_the_byte_where_a_failing_disk_diverges() {
        let source = image(10_000);
        let mut disk = source.clone();
        disk[7_777] ^= 0xFF;
        let err = verify(
            &mut source.as_slice(),
            &mut disk.as_slice(),
            10_000,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap_err();
        assert!(err.context().contains("7777"), "{err}");
    }

    #[test]
    fn verification_catches_a_stick_that_wraps_around() {
        // A counterfeit that reports 64 GB and has 8: everything past the real
        // capacity comes back as the start of the disk again.
        let source = image(CHUNK * 2);
        let mut disk = source.clone();
        let real = CHUNK;
        disk.truncate(real);
        disk.extend_from_within(..real);
        let err = verify(
            &mut source.as_slice(),
            &mut disk.as_slice(),
            (CHUNK * 2) as u64,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::ParseFailed);
    }

    #[test]
    fn verification_reads_only_the_image_not_the_padding() {
        // The disk is longer than the image. Comparing the padding would fail
        // against whatever was on the disk before.
        let source = image(3_000);
        let mut disk = source.clone();
        disk.extend_from_slice(&[0xAB; 72]);
        assert!(verify(
            &mut source.as_slice(),
            &mut disk.as_slice(),
            3_000,
            false,
            &mut nothing,
            &CancellationToken::never(),
        )
        .is_ok());
    }

    #[test]
    fn a_plan_refuses_an_image_that_does_not_exist() {
        let device = Device {
            node: PathBuf::from("/dev/null"),
            model: "test".into(),
            size: 1 << 30,
            bus: Bus::Usb,
            volumes: Vec::new(),
        };
        let err = Plan::new(Path::new("/nowhere/at/all.iso"), device).unwrap_err();
        assert_eq!(err.code(), ErrorCode::NotFound);
    }

    #[test]
    fn a_plan_refuses_a_directory() {
        let device = Device {
            node: PathBuf::from("/dev/null"),
            model: "test".into(),
            size: 1 << 30,
            bus: Bus::Usb,
            volumes: Vec::new(),
        };
        let err = Plan::new(Path::new("/"), device).unwrap_err();
        assert_eq!(err.code(), ErrorCode::WrongKind);
    }

    #[test]
    fn padding_rounds_up_and_leaves_exact_multiples_alone() {
        assert_eq!(pad_to_sector(0), 0);
        assert_eq!(pad_to_sector(1), 512);
        assert_eq!(pad_to_sector(512), 512);
        assert_eq!(pad_to_sector(513), 1_024);
    }

    /// A reader that behaves like a raw device: whole sectors or nothing.
    struct SectorsOnly<'a>(&'a [u8]);

    impl Read for SectorsOnly<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !buf.len().is_multiple_of(512) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "not a sector",
                ));
            }
            self.0.read(buf)
        }
    }

    /// A raw device refuses a read that is not whole sectors, so the last
    /// chunk is read padded - and the padding, which `copy` wrote as zeros past
    /// the image, is not compared.
    #[test]
    fn verifying_a_raw_disk_reads_whole_sectors_and_ignores_the_padding() {
        let image: Vec<u8> = (0..3_000_u32).map(|i| (i % 251) as u8).collect();
        let mut disk = Vec::new();
        let mut nothing = |_| {};
        copy(
            &mut image.as_slice(),
            &mut disk,
            3_000,
            true,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(disk.len(), 3_072, "padded to six sectors");

        let checked = verify(
            &mut image.as_slice(),
            &mut SectorsOnly(&disk),
            3_000,
            true,
            &mut nothing,
            &CancellationToken::never(),
        )
        .unwrap();
        assert_eq!(checked, 3_000);
    }

    fn image_file(name: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("jtf-declared-{}-{name}", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// The Steam Deck repair image that was cut short: a GPT whose backup header
    /// is at sector 15,859,711, in a file of a few kilobytes.
    #[test]
    fn a_gpt_says_how_long_its_disk_is() {
        let mut bytes = vec![0_u8; 4096];
        bytes[510] = 0x55;
        bytes[511] = 0xAA;
        bytes[446 + 4] = 0xEE; // the protective MBR entry
        bytes[512..520].copy_from_slice(b"EFI PART");
        bytes[512 + 32..512 + 40].copy_from_slice(&15_859_711_u64.to_le_bytes());
        let path = image_file("gpt", &bytes);
        let declared = declared_size(&path).unwrap().expect("a GPT");
        assert_eq!(declared.by, Layout::Gpt);
        assert_eq!(declared.needs, 8_120_172_544);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_mbr_says_where_its_last_partition_ends() {
        let mut bytes = vec![0_u8; 1024];
        bytes[510] = 0x55;
        bytes[511] = 0xAA;
        // Two partitions: 2048 + 1000 sectors, and 4096 + 2048 sectors.
        for (slot, start, count) in [(0_usize, 2048_u32, 1000_u32), (1, 4096, 2048)] {
            let entry = 446 + slot * 16;
            bytes[entry + 4] = 0x83;
            bytes[entry + 8..entry + 12].copy_from_slice(&start.to_le_bytes());
            bytes[entry + 12..entry + 16].copy_from_slice(&count.to_le_bytes());
        }
        let path = image_file("mbr", &bytes);
        let declared = declared_size(&path).unwrap().expect("an MBR");
        assert_eq!(declared.by, Layout::Mbr);
        assert_eq!(declared.needs, (4096 + 2048) * 512);
        let _ = std::fs::remove_file(path);
    }

    /// A filesystem written straight onto a stick carries 55 AA and boot code
    /// where a partition table would be. Code read as partitions gives
    /// nonsense, so it is not read as one.
    #[test]
    fn boot_code_is_not_mistaken_for_a_partition_table() {
        let mut bytes: Vec<u8> = (0..1024_u32).map(|i| (i * 37 % 253) as u8).collect();
        bytes[446] = 0x33; // not a boot flag
        bytes[510] = 0x55;
        bytes[511] = 0xAA;
        let path = image_file("fat", &bytes);
        assert_eq!(declared_size(&path).unwrap(), None);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_iso_says_how_many_blocks_it_has() {
        let mut bytes = vec![0_u8; 16 * 2048 + 2048];
        let pvd = 16 * 2048;
        bytes[pvd] = 1;
        bytes[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
        bytes[pvd + 80..pvd + 84].copy_from_slice(&300_000_u32.to_le_bytes());
        bytes[pvd + 128..pvd + 130].copy_from_slice(&2048_u16.to_le_bytes());
        let path = image_file("iso", &bytes);
        let declared = declared_size(&path).unwrap().expect("an ISO");
        assert_eq!(declared.by, Layout::Iso9660);
        assert_eq!(declared.needs, 300_000 * 2048);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_file_with_no_map_says_nothing() {
        let path = image_file("plain", b"just some bytes");
        assert_eq!(declared_size(&path).unwrap(), None);
        let _ = std::fs::remove_file(path);
    }
}
