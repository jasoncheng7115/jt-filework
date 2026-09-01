//! Extraction refuses what it is supposed to refuse.
//!
//! `docs/adr/0003-archive-extraction.md` names the corpus: `../` traversal,
//! absolute paths, drive-relative Windows paths, and names that differ only by
//! normalisation. A member name is written by whoever built the archive, so
//! every one of these is input from an attacker.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use jtf_fs::extract_archive;
use jtf_jobs::CancellationToken;

fn scratch(tag: &str) -> PathBuf {
    static SERIAL: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "jtf-archive-{tag}-{}-{nanos}-{serial}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

/// A ZIP whose members are named exactly as given, with no sanitising.
fn archive_with(root: &std::path::Path, names: &[&str]) -> PathBuf {
    let path = root.join("hostile.zip");
    let file = File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        // `start_file` keeps the name verbatim, which is the point.
        zip.start_file(*name, options).unwrap();
        zip.write_all(b"payload").unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn a_member_that_would_escape_is_refused_and_nothing_is_written_outside() {
    let root = scratch("escape");
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let destination = root.join("into");

    let archive = archive_with(
        &root,
        &[
            "../escaped.txt",
            "../../further.txt",
            "a/../../sideways.txt",
            "/absolute.txt",
            "..\\windows-style.txt",
            "ok.txt",
        ],
    );

    let done =
        extract_archive(&archive, &destination, &CancellationToken::never(), |_| {}).unwrap();

    assert_eq!(done.files, 1, "only the one safe member is written");
    assert!(destination.join("ok.txt").is_file());
    assert!(done.refused >= 4, "the rest are refused: {done:?}");

    // And nothing landed anywhere above the destination.
    for stray in [
        root.join("escaped.txt"),
        root.join("further.txt"),
        root.join("sideways.txt"),
        root.join("windows-style.txt"),
        outside.join("escaped.txt"),
    ] {
        assert!(!stray.exists(), "{} was written outside", stray.display());
    }
    let _ = fs::remove_dir_all(&root);
}

/// A name that is only a traversal once backslashes are read as separators.
///
/// On Windows `a\..\..\b` escapes; on Unix it is one odd filename. Treated as
/// a separator on both, because an archive is portable and the refusal should
/// be too.
#[test]
fn a_backslash_name_is_treated_as_a_path_on_every_platform() {
    let root = scratch("backslash");
    let destination = root.join("into");
    let archive = archive_with(&root, &["a\\..\\..\\escaped.txt"]);

    let done =
        extract_archive(&archive, &destination, &CancellationToken::never(), |_| {}).unwrap();

    assert_eq!(done.files, 0);
    assert_eq!(done.refused, 1);
    assert!(!root.join("escaped.txt").exists());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_directories_are_created_and_their_members_land_inside() {
    let root = scratch("nested");
    let destination = root.join("into");
    let archive = archive_with(&root, &["deep/inner/file.txt", "deep/other.txt"]);

    let done =
        extract_archive(&archive, &destination, &CancellationToken::never(), |_| {}).unwrap();

    assert_eq!(done.files, 2);
    assert_eq!(done.refused, 0);
    assert_eq!(
        fs::read(destination.join("deep/inner/file.txt")).unwrap(),
        b"payload"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_cancelled_extraction_reports_cancelled_and_leaves_no_partial_file() {
    let root = scratch("cancel");
    let destination = root.join("into");
    let archive = archive_with(&root, &["one.txt", "two.txt"]);

    let (token, canceller) = CancellationToken::new();
    canceller.cancel();
    let outcome = extract_archive(&archive, &destination, &token, |_| {});

    assert!(outcome.is_err(), "a cancelled extraction is not a success");
    assert!(!destination.join("one.txt").exists());
    let _ = fs::remove_dir_all(&root);
}

/// Round trip: what `create` writes, `extract` reads back unchanged.
#[test]
fn a_created_archive_extracts_to_what_went_in() {
    let root = scratch("roundtrip");
    let source = root.join("src");
    fs::create_dir_all(source.join("sub")).unwrap();
    fs::write(source.join("top.txt"), b"top").unwrap();
    fs::write(source.join("sub/inner.txt"), b"inner").unwrap();

    let archive = root.join("made.zip");
    let added = jtf_fs::create_archive(
        &archive,
        std::slice::from_ref(&source),
        &CancellationToken::never(),
        |_| {},
    )
    .unwrap();
    assert_eq!(added, 2);

    let back = root.join("back");
    let done = extract_archive(&archive, &back, &CancellationToken::never(), |_| {}).unwrap();
    assert_eq!(done.files, 2);
    assert_eq!(fs::read(back.join("src/top.txt")).unwrap(), b"top");
    assert_eq!(fs::read(back.join("src/sub/inner.txt")).unwrap(), b"inner");
    let _ = fs::remove_dir_all(&root);
}
