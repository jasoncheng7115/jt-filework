//! Symlink handling.
//!
//! Gated to Unix because creating a symlink on Windows needs a privilege that
//! CI runners do not reliably have; the Windows equivalent lands with the
//! Windows adapter in Phase 4 (`docs/TESTING.md` §5.3).

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use jtf_core::{FileEntry, FileKind, Location};
use jtf_fs::{LocalProvider, Provider};
use jtf_jobs::CancellationToken;

fn fixture() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!("jtf-symlink-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn a_symlink_is_reported_as_a_symlink_not_as_its_target() {
    // docs/SECURITY.md 3.1: a symlink must never be silently followed. If the
    // model reported the target's kind, a recursive delete could walk out of
    // the tree it was told to delete.
    let dir = fixture();
    fs::write(dir.join("real.txt"), b"x").unwrap();
    fs::create_dir(dir.join("realdir")).unwrap();
    std::os::unix::fs::symlink(dir.join("real.txt"), dir.join("to-file")).unwrap();
    std::os::unix::fs::symlink(dir.join("realdir"), dir.join("to-dir")).unwrap();
    std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("broken")).unwrap();

    let entries = LocalProvider::new()
        .list(&Location::local(&dir), &CancellationToken::never())
        .unwrap();

    let kind = |name: &str| {
        let Some(entry) = entries.iter().find(|e| e.display_name() == name) else {
            panic!("{name} missing")
        };
        FileEntry::kind(entry)
    };

    assert_eq!(kind("to-file"), FileKind::Symlink);
    assert_eq!(
        kind("to-dir"),
        FileKind::Symlink,
        "a link to a directory is still a link"
    );
    assert_eq!(
        kind("broken"),
        FileKind::Symlink,
        "a broken link is a link, not a missing row"
    );
    assert_eq!(kind("real.txt"), FileKind::File);
    assert_eq!(kind("realdir"), FileKind::Directory);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_symlink_cycle_does_not_hang_enumeration() {
    let dir = fixture();
    fs::create_dir(dir.join("a")).unwrap();
    std::os::unix::fs::symlink(&dir, dir.join("a/loop")).unwrap();

    let entries = LocalProvider::new()
        .list(&Location::local(&dir), &CancellationToken::never())
        .unwrap();
    assert_eq!(
        entries.len(),
        1,
        "enumeration is one level deep and cannot recurse into a cycle"
    );

    let _ = fs::remove_dir_all(&dir);
}
