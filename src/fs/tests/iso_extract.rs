//! Taking files out of an ISO image, including the ones it must refuse.
//!
//! The images are built here rather than downloaded, for the same reason the
//! listing corpus is: the cases worth testing are the ones no real image
//! contains. A member whose name climbs out of the destination is the whole
//! reason extraction has a safety check, and it cannot be tested against a
//! valid image.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use jtf_fs::{extract_iso, extract_iso_members};
use jtf_jobs::CancellationToken;

const SECTOR: usize = 2048;
const PVD_SECTOR: usize = 16;
const TERMINATOR_SECTOR: usize = 17;
const ROOT_SECTOR: usize = 18;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("jtf-isox-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn directory_record(
    extent: u32,
    length: u32,
    is_directory: bool,
    name: &[u8],
    system_use: &[u8],
) -> Vec<u8> {
    let padding = usize::from(name.len().is_multiple_of(2));
    let total = 33 + name.len() + padding + system_use.len();
    let mut record = vec![0_u8; total];
    record[0] = u8::try_from(total).expect("record fits in a byte");
    record[2..6].copy_from_slice(&extent.to_le_bytes());
    record[6..10].copy_from_slice(&extent.to_be_bytes());
    record[10..14].copy_from_slice(&length.to_le_bytes());
    record[14..18].copy_from_slice(&length.to_be_bytes());
    record[25] = if is_directory { 0x02 } else { 0x00 };
    record[32] = u8::try_from(name.len()).expect("name length fits in a byte");
    record[33..33 + name.len()].copy_from_slice(name);
    if !system_use.is_empty() {
        let at = 33 + name.len() + padding;
        record[at..at + system_use.len()].copy_from_slice(system_use);
    }
    record
}

/// An image holding `files` in its root, each as (name, contents), plus an
/// optional Rock Ridge name for the first of them.
fn image_with(files: &[(&[u8], &[u8])], rock_ridge_first: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = vec![0_u8; 32 * SECTOR];
    let sector_len = u32::try_from(SECTOR).unwrap();

    // Primary volume descriptor.
    let pvd = PVD_SECTOR * SECTOR;
    bytes[pvd] = 1;
    bytes[pvd + 1..pvd + 6].copy_from_slice(b"CD001");
    bytes[pvd + 6] = 1;
    let root = directory_record(
        u32::try_from(ROOT_SECTOR).unwrap(),
        sector_len,
        true,
        &[0],
        &[],
    );
    bytes[pvd + 156..pvd + 156 + root.len()].copy_from_slice(&root);

    // Terminator.
    let end = TERMINATOR_SECTOR * SECTOR;
    bytes[end] = 255;
    bytes[end + 1..end + 6].copy_from_slice(b"CD001");
    bytes[end + 6] = 1;

    // Root directory: `.`, `..`, then the files, each in its own sector.
    let mut records = vec![
        directory_record(
            u32::try_from(ROOT_SECTOR).unwrap(),
            sector_len,
            true,
            &[0],
            &[],
        ),
        directory_record(
            u32::try_from(ROOT_SECTOR).unwrap(),
            sector_len,
            true,
            &[1],
            &[],
        ),
    ];
    for (index, (name, content)) in files.iter().enumerate() {
        let sector = u32::try_from(20 + index).unwrap();
        let system_use = if index == 0 {
            rock_ridge_first.map_or_else(Vec::new, |posix| {
                let mut nm = vec![b'N', b'M', u8::try_from(5 + posix.len()).unwrap(), 1, 0];
                nm.extend_from_slice(posix);
                nm
            })
        } else {
            Vec::new()
        };
        records.push(directory_record(
            sector,
            u32::try_from(content.len()).unwrap(),
            false,
            name,
            &system_use,
        ));
        let at = sector as usize * SECTOR;
        bytes[at..at + content.len()].copy_from_slice(content);
    }

    let mut at = ROOT_SECTOR * SECTOR;
    for record in &records {
        bytes[at..at + record.len()].copy_from_slice(record);
        at += record.len();
    }
    bytes
}

fn write_image(dir: &Path, bytes: &[u8]) -> PathBuf {
    let path = dir.join("disc.iso");
    std::fs::write(&path, bytes).expect("write image");
    path
}

#[test]
fn extracts_every_file_with_its_contents() {
    let dir = temp_dir("all");
    let image = write_image(
        &dir,
        &image_with(
            &[
                (b"ONE.TXT;1", b"first" as &[u8]),
                (b"TWO.TXT;1", b"second" as &[u8]),
            ],
            None,
        ),
    );
    let out = dir.join("out");

    let done = extract_iso(&image, &out, &CancellationToken::never(), |_| {}).expect("extracted");
    assert_eq!(done.files, 2);
    assert_eq!(done.refused, 0);
    assert_eq!(
        std::fs::read_to_string(out.join("ONE.TXT")).expect("one"),
        "first"
    );
    assert_eq!(
        std::fs::read_to_string(out.join("TWO.TXT")).expect("two"),
        "second"
    );
}

/// `C` in the archive window takes what is marked; the rest is untouched, not
/// refused - it was never a candidate.
#[test]
fn extracts_only_the_named_members() {
    let dir = temp_dir("some");
    let image = write_image(
        &dir,
        &image_with(
            &[
                (b"WANTED.TXT;1", b"yes" as &[u8]),
                (b"OTHER.TXT;1", b"no" as &[u8]),
            ],
            None,
        ),
    );
    let out = dir.join("out");

    let done = extract_iso_members(
        &image,
        &out,
        &["WANTED.TXT".to_string()],
        &CancellationToken::never(),
        |_| {},
    )
    .expect("extracted");
    assert_eq!(done.files, 1);
    assert_eq!(done.refused, 0, "an unmarked member is not a refusal");
    assert!(out.join("WANTED.TXT").exists());
    assert!(!out.join("OTHER.TXT").exists());
}

/// A name that climbs out of the destination is refused and counted, never
/// written and never quietly renamed.
#[test]
fn a_traversal_member_is_refused_and_nothing_lands_outside() {
    let dir = temp_dir("traversal");
    let image = write_image(
        &dir,
        &image_with(
            &[(b"PASSWD;1", b"owned" as &[u8])],
            Some(b"../../escaped.txt"),
        ),
    );
    let out = dir.join("out");

    let done = extract_iso(&image, &out, &CancellationToken::never(), |_| {}).expect("ran");
    assert_eq!(done.files, 0, "nothing should have been written");
    assert_eq!(done.refused, 1, "the refusal must be counted, not silent");
    assert!(
        !dir.join("escaped.txt").exists(),
        "a file landed outside the destination"
    );
    assert!(
        !dir.parent().unwrap().join("escaped.txt").exists(),
        "a file landed two levels outside the destination"
    );
}

/// Cancelling leaves no half-written file pretending to be extracted.
#[test]
fn cancelling_writes_nothing() {
    let dir = temp_dir("cancel");
    let image = write_image(
        &dir,
        &image_with(&[(b"BIG.TXT;1", b"content" as &[u8])], None),
    );
    let out = dir.join("out");

    let outcome = extract_iso(&image, &out, &CancellationToken::cancelled(), |_| {});
    assert!(outcome.is_err(), "a cancelled extraction is not a success");
    assert!(!out.join("BIG.TXT").exists());
}

/// A file that is not an image is refused, rather than producing an empty
/// destination that looks like an image with nothing in it.
#[test]
fn a_file_that_is_not_an_image_is_refused() {
    let dir = temp_dir("notiso");
    let path = dir.join("plain.iso");
    std::fs::write(&path, b"this is not a disc image").expect("write");
    let out = dir.join("out");

    assert!(extract_iso(&path, &out, &CancellationToken::never(), |_| {}).is_err());
}
