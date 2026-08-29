//! A filename on Unix is a byte string, not text. jt-filework must carry it
//! through unchanged while still showing the user something readable
//! (`docs/SECURITY.md` §3, `docs/TESTING.md` §9.2).
//!
//! Gated to Unix because Windows filenames are UTF-16 and cannot express this
//! case; the Windows equivalent (unpaired surrogates) gets its own test when
//! the Windows adapter lands in Phase 4.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use jtf_core::{FileEntry, FileKind, Location, RawName};

#[test]
fn a_non_utf8_name_survives_intact_and_still_displays() {
    let raw_bytes = vec![b'b', b'a', b'd', 0xFF, b'.', b't', b'x', b't'];
    let invalid = OsString::from_vec(raw_bytes.clone());

    let entry = FileEntry::new(
        Location::local("/tmp/x"),
        RawName::new(invalid.clone()),
        FileKind::File,
    );

    assert_eq!(
        entry.raw_name().as_os_str(),
        invalid.as_os_str(),
        "raw bytes preserved"
    );
    assert!(entry.raw_name().is_lossy_as_utf8());
    assert_eq!(
        entry.raw_name().to_str(),
        None,
        "must not pretend it is UTF-8"
    );

    let shown = entry.display_name();
    assert!(
        shown.contains('\u{FFFD}'),
        "display form replaces the invalid byte"
    );
    assert_ne!(
        shown.as_bytes(),
        raw_bytes.as_slice(),
        "display form is not the raw name"
    );
}
