//! Against an archive this program did not write.
//!
//! The round trip in `archive_safety.rs` proves we can read what we wrote,
//! which is the weaker claim. This one uses a ZIP produced by the platform's
//! own `zip`, so a disagreement about the format shows up here rather than in
//! front of a user.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use jtf_fs::extract_archive;
use jtf_jobs::CancellationToken;

#[test]
fn a_zip_written_by_the_system_tool_extracts_intact() {
    let root = std::env::temp_dir().join(format!("jtf-realzip-{}", std::process::id()));
    let source = root.join("src");
    fs::create_dir_all(source.join("sub")).unwrap();
    fs::write(source.join("top.txt"), b"hello\n").unwrap();
    fs::write(source.join("sub/inner.txt"), b"inner\n").unwrap();

    let archive: PathBuf = root.join("made.zip");
    let made = Command::new("zip")
        .arg("-q")
        .arg("-r")
        .arg(&archive)
        .arg("src")
        .current_dir(&root)
        .status();
    let Ok(status) = made else {
        eprintln!("skipped: no `zip` on this machine");
        let _ = fs::remove_dir_all(&root);
        return;
    };
    if !status.success() {
        eprintln!("skipped: `zip` refused to run");
        let _ = fs::remove_dir_all(&root);
        return;
    }

    let out = root.join("out");
    let done = extract_archive(&archive, &out, &CancellationToken::never(), |_| {}).unwrap();

    assert_eq!(
        done.refused, 0,
        "nothing in a plain archive should be refused"
    );
    assert_eq!(fs::read(out.join("src/top.txt")).unwrap(), b"hello\n");
    assert_eq!(fs::read(out.join("src/sub/inner.txt")).unwrap(), b"inner\n");
    let _ = fs::remove_dir_all(&root);
}
