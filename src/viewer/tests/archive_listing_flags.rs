//! The listing marks a member that would escape, rather than hiding it.
//!
//! The archive window shows this flag and says so on hover, and extraction
//! refuses the member either way. Seeing it is the point: an archive that
//! contains a traversal is worth knowing about before you extract it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

#[test]
fn a_traversal_member_is_listed_and_flagged() {
    let root = std::env::temp_dir().join(format!("jtf-listflags-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let archive: PathBuf = root.join("hostile.zip");

    {
        let file = File::create(&archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for name in ["normal.txt", "deep/inner.txt", "../escaped.txt"] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"payload").unwrap();
        }
        zip.finish().unwrap();
    }

    let entries = jtf_viewer::list_archive(&archive).expect("the listing reads");
    let flagged: Vec<&str> = entries
        .iter()
        .filter(|e| e.unsafe_name)
        .map(|e| e.name.as_str())
        .collect();

    assert_eq!(
        flagged,
        vec!["../escaped.txt"],
        "exactly the traversal is flagged, and it is still listed: {entries:?}"
    );
    assert_eq!(entries.len(), 3, "nothing is hidden from the listing");
    let _ = fs::remove_dir_all(&root);
}
