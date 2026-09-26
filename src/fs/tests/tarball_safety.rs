//! tar and the stream compressors, against archives built here (ADR-0006).
//!
//! The archives are assembled in the test rather than downloaded, because the
//! cases worth testing are the ones no real archive contains: a member that
//! climbs out of the destination, a symlink, a name with a Windows drive on
//! it. A valid `.tar.gz` proves the happy path and nothing else.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use jtf_fs::{create_tarball, extract_tarball_members, tar_kind, Compression};
use jtf_jobs::CancellationToken;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jtf-tar-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, contents).expect("write");
}

/// A 512-byte tar header, written by hand.
///
/// The `tar` crate's writer refuses `..` and absolute paths - which is right
/// for a writer and useless for this test. An attacker does not use our
/// writer, so the bytes are laid out here the way a hostile archive really
/// arrives.
fn tar_header(name: &str, size: usize) -> [u8; 512] {
    let mut header = [0_u8; 512];
    let name_bytes = name.as_bytes();
    header[..name_bytes.len().min(100)].copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);
    header[100..107].copy_from_slice(b"0000644"); // mode
    header[108..115].copy_from_slice(b"0000000"); // uid
    header[116..123].copy_from_slice(b"0000000"); // gid
    let size_field = format!("{size:011o}");
    header[124..135].copy_from_slice(size_field.as_bytes());
    header[136..147].copy_from_slice(b"00000000000"); // mtime
    header[156] = b'0'; // a regular file
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    // The checksum is computed with its own field read as spaces.
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    let checksum = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(checksum.as_bytes());
    header
}

/// A gzip-compressed tar holding one member with exactly the name given.
fn gz_with_member(at: &Path, name: &str, body: &str) -> PathBuf {
    let path = at.join("hostile.tar.gz");
    let file = fs::File::create(&path).expect("create");
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());

    encoder
        .write_all(&tar_header(name, body.len()))
        .expect("header");
    let mut block = vec![0_u8; 512];
    block[..body.len()].copy_from_slice(body.as_bytes());
    encoder.write_all(&block).expect("body");
    // Two empty blocks end an archive.
    encoder.write_all(&[0_u8; 1024]).expect("end");
    encoder.finish().expect("finish");
    path
}

#[test]
fn creates_and_reads_back_a_tar_gz() {
    let dir = temp_dir("roundtrip");
    write(&dir.join("src/one.txt"), "first");
    write(&dir.join("src/deeper/two.txt"), "second");

    let archive = dir.join("out.tar.gz");
    let count = create_tarball(
        &archive,
        &[dir.join("src")],
        Compression::Gzip,
        &CancellationToken::never(),
        |_| {},
    )
    .expect("created");
    assert_eq!(count, 1, "one source was added");

    let kind = tar_kind(&archive).expect("recognised");
    assert_eq!(kind.compression, Compression::Gzip);
    assert!(kind.is_tar, "a tar should have been found inside");

    let out = dir.join("back");
    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("extracted");
    assert_eq!(done.refused, 0);
    assert_eq!(
        fs::read_to_string(out.join("src/one.txt")).unwrap(),
        "first"
    );
    assert_eq!(
        fs::read_to_string(out.join("src/deeper/two.txt")).unwrap(),
        "second"
    );
}

/// The format is decided by the bytes, not by the name.
#[test]
fn a_tar_gz_named_anything_is_still_recognised() {
    let dir = temp_dir("byname");
    write(&dir.join("src/a.txt"), "x");
    let archive = dir.join("no-extension-at-all");
    create_tarball(
        &archive,
        &[dir.join("src")],
        Compression::Gzip,
        &CancellationToken::never(),
        |_| {},
    )
    .expect("created");
    let kind = tar_kind(&archive).expect("recognised by content");
    assert_eq!(kind.compression, Compression::Gzip);
    assert!(kind.is_tar);
}

/// A plain file is not an archive, whatever it is called.
#[test]
fn something_that_is_not_an_archive_is_not_claimed() {
    let dir = temp_dir("notarchive");
    let path = dir.join("pretend.tar.gz");
    write(&path, "this is just text");
    assert!(tar_kind(&path).is_none());
}

/// A member whose name climbs out of the destination is refused and counted,
/// never written and never quietly renamed.
#[test]
fn a_traversal_member_is_refused_and_nothing_lands_outside() {
    let dir = temp_dir("traversal");
    let archive = gz_with_member(&dir, "../../escaped.txt", "owned");
    let out = dir.join("out");

    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("ran");
    assert_eq!(done.files, 0, "nothing should have been written");
    assert_eq!(done.refused, 1, "the refusal must be counted, not silent");
    assert!(!dir.join("escaped.txt").exists());
    assert!(!dir.parent().unwrap().join("escaped.txt").exists());
}

/// An absolute name is the same attack spelled differently.
#[test]
fn an_absolute_member_name_is_refused() {
    let dir = temp_dir("absolute");
    let archive = gz_with_member(&dir, "/tmp/jtf-should-not-exist.txt", "owned");
    let out = dir.join("out");

    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("ran");
    assert_eq!(done.files, 0);
    assert!(!Path::new("/tmp/jtf-should-not-exist.txt").exists());
}

/// `C:\Windows\...` is a traversal on Windows and a legal, if odd, filename
/// on Unix. What must hold on both is the same thing: nothing lands outside
/// the folder the user chose. Windows refuses it outright; Unix keeps it
/// inside as a directory literally called `C:`.
#[test]
fn a_windows_drive_name_never_escapes() {
    let dir = temp_dir("drive");
    let archive = gz_with_member(&dir, r"C:\Windows\owned.txt", "owned");
    let out = dir.join("out");

    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("ran");
    assert!(
        !Path::new("C:\\Windows\\owned.txt").exists(),
        "a file landed at a drive-absolute path"
    );
    assert!(!dir.join("Windows/owned.txt").exists());
    assert!(!dir.parent().unwrap().join("owned.txt").exists());
    if cfg!(windows) {
        assert_eq!(done.refused, 1, "Windows must refuse a drive prefix");
    } else {
        // Contained, under a folder named after the drive. Written, but
        // nowhere it could do harm.
        assert!(out.exists());
    }
}

/// `C` in the archive window takes what is marked; the rest is untouched, not
/// refused - it was never a candidate.
#[test]
fn extracts_only_the_named_members() {
    let dir = temp_dir("some");
    write(&dir.join("src/wanted.txt"), "yes");
    write(&dir.join("src/other.txt"), "no");
    let archive = dir.join("both.tar.gz");
    create_tarball(
        &archive,
        &[dir.join("src")],
        Compression::Gzip,
        &CancellationToken::never(),
        |_| {},
    )
    .expect("created");

    let out = dir.join("out");
    let done = extract_tarball_members(
        &archive,
        &out,
        &["src/wanted.txt".to_string()],
        &CancellationToken::never(),
        |_| {},
    )
    .expect("extracted");
    assert_eq!(done.refused, 0, "an unmarked member is not a refusal");
    assert!(out.join("src/wanted.txt").exists());
    assert!(!out.join("src/other.txt").exists());
}

/// A link inside an archive is refused rather than created: writing one means
/// a later write through it lands wherever the archive chose.
#[cfg(unix)]
#[test]
fn a_symlink_member_is_refused() {
    let dir = temp_dir("link");
    let archive = dir.join("link.tar.gz");
    let file = fs::File::create(&archive).expect("create");
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_link_name("/etc/passwd").expect("link name");
    header.set_cksum();
    builder
        .append_data(&mut header, "innocent.txt", std::io::empty())
        .expect("append");
    builder
        .into_inner()
        .and_then(flate2::write::GzEncoder::finish)
        .expect("finish");

    let out = dir.join("out");
    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("ran");
    assert_eq!(done.refused, 1, "a link member must be refused");
    assert!(!out.join("innocent.txt").exists());
}

/// Cancelling leaves no half-written file pretending to be extracted.
#[test]
fn cancelling_writes_nothing() {
    let dir = temp_dir("cancel");
    write(&dir.join("src/a.txt"), "content");
    let archive = dir.join("c.tar.gz");
    create_tarball(
        &archive,
        &[dir.join("src")],
        Compression::Gzip,
        &CancellationToken::never(),
        |_| {},
    )
    .expect("created");

    let out = dir.join("out");
    let outcome =
        extract_tarball_members(&archive, &out, &[], &CancellationToken::cancelled(), |_| {});
    assert!(outcome.is_err(), "a cancelled extraction is not a success");
    assert!(!out.join("src/a.txt").exists());
}

/// Writing bzip2 or xz is refused rather than half-attempted: this build
/// reads them and does not write them, and saying so is better than producing
/// something that is not what its name says (ADR-0006).
#[test]
fn creating_bzip2_or_xz_is_refused_plainly() {
    let dir = temp_dir("nowrite");
    write(&dir.join("src/a.txt"), "x");
    for compression in [Compression::Bzip2, Compression::Xz] {
        assert!(!compression.can_write());
        let archive = dir.join(format!("out.{}", compression.extension()));
        assert!(create_tarball(
            &archive,
            &[dir.join("src")],
            compression,
            &CancellationToken::never(),
            |_| {},
        )
        .is_err());
    }
}

/// A bare `.gz` holds one file, not an archive, and unwraps to its own name
/// without the suffix.
#[test]
fn a_bare_gzip_unwraps_to_one_file() {
    let dir = temp_dir("bare");
    let archive = dir.join("notes.txt.gz");
    let out_file = fs::File::create(&archive).expect("create");
    let mut encoder = flate2::write::GzEncoder::new(out_file, flate2::Compression::default());
    encoder.write_all(b"just some text").expect("write");
    encoder.finish().expect("finish");

    let kind = tar_kind(&archive).expect("recognised");
    assert_eq!(kind.compression, Compression::Gzip);
    assert!(!kind.is_tar, "a bare gzip holds no tar");

    let out = dir.join("out");
    let done = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("extracted");
    assert_eq!(done.files, 1);
    assert_eq!(
        fs::read_to_string(out.join("notes.txt")).unwrap(),
        "just some text"
    );
}

/// Text that compresses, but not to nothing: enough blocks that cutting the
/// stream in half cuts through the middle of one.
fn sample_text(bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes);
    let mut n: u32 = 1;
    while out.len() < bytes {
        n = n.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        out.extend_from_slice(format!("line {n:08x} of a log nobody reads\n").as_bytes());
    }
    out.truncate(bytes);
    out
}

fn bzip2_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    encoder.write_all(data).expect("compress");
    encoder.finish().expect("finish")
}

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).expect("compress");
    encoder.finish().expect("finish")
}

fn xz_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    lzma_rs::xz_compress(&mut &data[..], &mut out).expect("compress");
    out
}

/// Every file under `dir`, so a test can say nothing else was left there.
fn files_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// A `.bz2` from a parallel compressor is several complete streams one after
/// another, and all of them are the file.
///
/// A decoder that stops at the end of the first stream reports a clean end of
/// file, and what it wrote looks like the whole thing.
#[test]
fn a_bzip2_of_several_streams_unwraps_all_of_them() {
    let dir = temp_dir("bz2-multi");
    let first = sample_text(300_000);
    let second = sample_text(200_000);
    let mut joined = bzip2_bytes(&first);
    joined.extend_from_slice(&bzip2_bytes(&second));
    let archive = dir.join("disk.img.bz2");
    fs::write(&archive, &joined).expect("write");

    let out = dir.join("out");
    extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {})
        .expect("extracted");
    let got = fs::read(out.join("disk.img")).expect("read back");
    assert_eq!(got.len(), first.len() + second.len());
    assert_eq!(&got[..first.len()], &first[..]);
    assert_eq!(&got[first.len()..], &second[..]);
    assert_eq!(files_in(&out), ["disk.img"], "no part file left behind");
}

/// A stream cut short - a download that did not finish - fails, and leaves
/// nothing under the real name or any other.
///
/// Until 0.6.54 a decoder error left what it had written so far, named as if
/// it were the whole file: 324 MB of an 8 GB disk image, which was then
/// written to a USB stick.
#[test]
fn a_compressed_file_cut_short_fails_and_leaves_nothing() {
    let data = sample_text(2_000_000);
    for (suffix, compressed) in [
        ("bz2", bzip2_bytes(&data)),
        ("gz", gzip_bytes(&data)),
        ("xz", xz_bytes(&data)),
    ] {
        let dir = temp_dir(&format!("cut-{suffix}"));
        let archive = dir.join(format!("disk.img.{suffix}"));
        fs::write(&archive, &compressed[..compressed.len() / 2]).expect("write");

        let out = dir.join("out");
        let outcome =
            extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {});
        assert!(
            outcome.is_err(),
            ".{suffix} cut in half was reported as extracted"
        );
        assert!(
            files_in(&out).is_empty(),
            ".{suffix}: left behind {:?}",
            files_in(&out)
        );
    }
}

/// Damage in the middle of a stream fails the same way, rather than
/// producing a file with a hole in it.
///
/// Not xz: what makes damage detectable there is the check the `xz` tool
/// writes after every block (CRC64 by default), which the decoder verifies -
/// and `lzma-rs`'s own compressor, the only one a test can use, writes none.
/// A stream with no check decodes damage into different bytes, and nothing
/// can tell those from the file's own.
#[test]
fn a_damaged_compressed_file_fails_and_leaves_nothing() {
    let data = sample_text(2_000_000);
    for (suffix, mut compressed) in [("bz2", bzip2_bytes(&data)), ("gz", gzip_bytes(&data))] {
        let middle = compressed.len() / 2;
        for byte in &mut compressed[middle..middle + 64] {
            *byte ^= 0x5a;
        }
        let dir = temp_dir(&format!("damaged-{suffix}"));
        let archive = dir.join(format!("disk.img.{suffix}"));
        fs::write(&archive, &compressed).expect("write");

        let out = dir.join("out");
        let outcome =
            extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {});
        assert!(
            outcome.is_err(),
            "damaged .{suffix} was reported as extracted"
        );
        assert!(
            files_in(&out).is_empty(),
            ".{suffix}: left behind {:?}",
            files_in(&out)
        );
    }
}

/// A `.tar.bz2` cut short in its second member keeps the first, which is
/// whole, drops the second, which is not, and says it failed.
#[test]
fn a_tar_cut_short_keeps_whole_members_only_and_fails() {
    let dir = temp_dir("tar-cut");
    let first = sample_text(50_000);
    let second = sample_text(900_000);
    let mut tar_bytes = Vec::new();
    for (name, body) in [("first.txt", &first), ("second.txt", &second)] {
        tar_bytes.extend_from_slice(&tar_header(name, body.len()));
        tar_bytes.extend_from_slice(body);
        let padding = (512 - body.len() % 512) % 512;
        tar_bytes.extend(std::iter::repeat_n(0_u8, padding));
    }
    tar_bytes.extend(std::iter::repeat_n(0_u8, 1024));
    let compressed = bzip2_bytes(&tar_bytes);
    let archive = dir.join("two.tar.bz2");
    fs::write(&archive, &compressed[..compressed.len() * 3 / 4]).expect("write");

    let out = dir.join("out");
    let outcome = extract_tarball_members(&archive, &out, &[], &CancellationToken::never(), |_| {});
    assert!(
        outcome.is_err(),
        "a truncated tar was reported as extracted"
    );
    assert_eq!(files_in(&out), ["first.txt"], "only the whole member stays");
    assert_eq!(fs::read(out.join("first.txt")).unwrap(), first);
}
