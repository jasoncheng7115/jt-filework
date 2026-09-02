//! Writing an image and reading it back, against real files.
//!
//! The disk is a file here. That is not a weaker test than a real stick for
//! everything except the privileged open: the engine reads with `Read`, writes
//! with `Write`, and pads to a sector because the raw node demands it — none of
//! which knows or cares what is on the other end. What a real stick adds is the
//! authorization prompt and the failure modes of cheap flash, and the second of
//! those is simulated below by handing back bytes that are wrong.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use jtf_imaging::{copy, verify, Crc32, CHUNK};
use jtf_jobs::{CancellationToken, Progress};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("jtf-imaging-tests");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn image_bytes(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| u8::try_from(i.wrapping_mul(31) % 251).unwrap_or(0))
        .collect()
}

fn nothing(_: Progress) {}

#[test]
fn an_image_written_to_a_disk_reads_back_identical() {
    let source_path = scratch("source.iso");
    let disk_path = scratch("disk.img");
    let bytes = image_bytes(CHUNK + 1_337);
    fs::write(&source_path, &bytes).unwrap();

    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::create(&disk_path).unwrap();
    let checksum = copy(
        &mut source,
        &mut disk,
        bytes.len() as u64,
        true,
        &mut nothing,
        &CancellationToken::never(),
    )
    .unwrap();
    disk.flush().unwrap();
    drop(disk);

    assert_eq!(checksum, Crc32::of(&bytes));

    // The disk is longer than the image, because the last write was padded up
    // to a sector. Verification must read only as far as the image goes.
    let written = fs::metadata(&disk_path).unwrap().len();
    assert!(written > bytes.len() as u64);
    assert_eq!(written % 512, 0, "the disk did not get whole sectors");

    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::open(&disk_path).unwrap();
    let checked = verify(
        &mut source,
        &mut disk,
        bytes.len() as u64,
        &mut nothing,
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!(checked, bytes.len() as u64);

    fs::remove_file(source_path).ok();
    fs::remove_file(disk_path).ok();
}

#[test]
fn a_disk_that_corrupts_one_byte_is_caught_and_the_offset_is_named() {
    let source_path = scratch("source-2.iso");
    let disk_path = scratch("disk-2.img");
    let bytes = image_bytes(200_000);
    fs::write(&source_path, &bytes).unwrap();
    fs::write(&disk_path, &bytes).unwrap();

    // What a stick with a bad cell does: it took the write and reports
    // something else.
    let mut disk = fs::OpenOptions::new().write(true).open(&disk_path).unwrap();
    disk.seek(SeekFrom::Start(123_456)).unwrap();
    disk.write_all(&[0x00]).unwrap();
    drop(disk);

    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::open(&disk_path).unwrap();
    let err = verify(
        &mut source,
        &mut disk,
        bytes.len() as u64,
        &mut nothing,
        &CancellationToken::never(),
    )
    .unwrap_err();
    assert!(
        err.context().contains("123456"),
        "the offset was not reported: {err}"
    );

    fs::remove_file(source_path).ok();
    fs::remove_file(disk_path).ok();
}

#[test]
fn a_disk_smaller_than_it_claims_fails_verification_rather_than_the_write() {
    // The counterfeit case, and the reason verification is on by default: the
    // write succeeds, every byte is accepted, and the disk holds a fraction of
    // what it was given.
    let source_path = scratch("source-3.iso");
    let disk_path = scratch("disk-3.img");
    let bytes = image_bytes(CHUNK * 2);
    fs::write(&source_path, &bytes).unwrap();

    // A "disk" that wraps: the second half is the first half again.
    let mut wrapped = bytes[..CHUNK].to_vec();
    wrapped.extend_from_slice(&bytes[..CHUNK]);
    fs::write(&disk_path, &wrapped).unwrap();

    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::open(&disk_path).unwrap();
    assert!(
        verify(
            &mut source,
            &mut disk,
            bytes.len() as u64,
            &mut nothing,
            &CancellationToken::never(),
        )
        .is_err(),
        "a disk that wraps around passed verification"
    );

    fs::remove_file(source_path).ok();
    fs::remove_file(disk_path).ok();
}

#[test]
fn cancelling_part_way_leaves_the_disk_short_and_says_so() {
    // There is no undo for this, and the test exists to pin what actually
    // happens: the write stops where it was. The UI has to say the disk is
    // unusable, because it is.
    let source_path = scratch("source-4.iso");
    let disk_path = scratch("disk-4.img");
    let bytes = image_bytes(CHUNK * 3);
    fs::write(&source_path, &bytes).unwrap();

    let (token, canceller) = CancellationToken::new();
    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::create(&disk_path).unwrap();
    let err = copy(
        &mut source,
        &mut disk,
        bytes.len() as u64,
        true,
        &mut |p| {
            if p.completed() >= CHUNK as u64 {
                canceller.cancel();
            }
        },
        &token,
    )
    .unwrap_err();
    assert_eq!(err.code(), jtf_core::ErrorCode::Cancelled);
    drop(disk);

    let on_disk = fs::metadata(&disk_path).unwrap().len();
    assert!(on_disk > 0 && on_disk < bytes.len() as u64, "{on_disk}");

    fs::remove_file(source_path).ok();
    fs::remove_file(disk_path).ok();
}

#[test]
fn an_image_that_is_exactly_one_chunk_needs_no_padding_and_no_second_pass() {
    let source_path = scratch("source-5.iso");
    let disk_path = scratch("disk-5.img");
    let bytes = image_bytes(CHUNK);
    fs::write(&source_path, &bytes).unwrap();

    let mut source = fs::File::open(&source_path).unwrap();
    let mut disk = fs::File::create(&disk_path).unwrap();
    copy(
        &mut source,
        &mut disk,
        bytes.len() as u64,
        true,
        &mut nothing,
        &CancellationToken::never(),
    )
    .unwrap();
    drop(disk);

    assert_eq!(fs::metadata(&disk_path).unwrap().len(), CHUNK as u64);
    let mut back = Vec::new();
    fs::File::open(&disk_path)
        .unwrap()
        .read_to_end(&mut back)
        .unwrap();
    assert_eq!(back, bytes);

    fs::remove_file(source_path).ok();
    fs::remove_file(disk_path).ok();
}
