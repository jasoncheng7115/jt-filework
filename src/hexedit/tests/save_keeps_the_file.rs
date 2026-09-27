//! Saving writes the bytes back into the same file, not merely a file with
//! the same bytes.
#![allow(clippy::unwrap_used)]

/// A script saved from the hex editor keeps its executable bit, and a private
/// file stays private. Saving goes through a temporary and a rename, and the
/// temporary is created with the process's default permissions - which the
/// file then had, until 0.6.56.
#[cfg(unix)]
#[test]
fn saving_keeps_the_files_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!("jtf-hexedit-perm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for mode in [0o755, 0o600] {
        let path = dir.join(format!("file-{mode:o}"));
        std::fs::write(&path, b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();

        let mut session = jtf_hexedit::Session::open(&path).unwrap();
        session.set_mode(jtf_hexedit::session::Mode::Overwrite);
        session.move_to(15, false);
        session.write(b"HI").unwrap();
        session.save().unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"#!/bin/sh\necho HI\n");
        let now = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(now, mode, "saved with {now:o}, was {mode:o}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
