//! ISO 9660 listing, against images built here rather than downloaded.
//!
//! An image is a file people download, so the interesting cases are not the
//! valid ones. Each image below is assembled byte by byte so a hostile field
//! can be written deliberately: a record pointing past the end of the file, a
//! directory that is its own parent, a name that climbs out of the extraction
//! folder, a length that does not fit.
//!
//! What the reader must never do with any of them is loop forever, allocate to
//! a size the file chose, or panic. Reporting nothing is an acceptable answer;
//! crashing is not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use jtf_viewer::{list_iso, read_iso};

const SECTOR: usize = 2048;

/// Sector 16 is the first volume descriptor; 17 is the terminator; the root
/// directory lands at 18 and any file content after that.
const PVD_SECTOR: usize = 16;
const TERMINATOR_SECTOR: usize = 17;
const ROOT_SECTOR: usize = 18;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jtf-iso-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(name: &str, bytes: &[u8]) -> PathBuf {
    let path = temp_dir().join(name);
    std::fs::write(&path, bytes).expect("write image");
    path
}

/// An image under construction: a flat vector of sectors.
struct Image {
    bytes: Vec<u8>,
}

impl Image {
    fn new(sectors: usize) -> Self {
        Self {
            bytes: vec![0_u8; sectors * SECTOR],
        }
    }

    fn put(&mut self, at: usize, bytes: &[u8]) {
        self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// A primary volume descriptor whose root record points at `root_sector`.
    fn primary(&mut self, root_sector: u32, root_length: u32) {
        let at = PVD_SECTOR * SECTOR;
        self.bytes[at] = 1; // primary
        self.put(at + 1, b"CD001");
        self.bytes[at + 6] = 1; // version
        let root = directory_record(root_sector, root_length, true, &[0], &[]);
        self.put(at + 156, &root);
    }

    /// A supplementary descriptor announcing Joliet.
    fn joliet(&mut self, sector: usize, root_sector: u32, root_length: u32) {
        let at = sector * SECTOR;
        self.bytes[at] = 2; // supplementary
        self.put(at + 1, b"CD001");
        self.bytes[at + 6] = 1;
        self.put(at + 88, b"%/E"); // the UCS-2 level 3 escape
        let root = directory_record(root_sector, root_length, true, &[0], &[]);
        self.put(at + 156, &root);
    }

    fn terminator(&mut self, sector: usize) {
        let at = sector * SECTOR;
        self.bytes[at] = 255;
        self.put(at + 1, b"CD001");
        self.bytes[at + 6] = 1;
    }

    fn directory(&mut self, sector: usize, records: &[Vec<u8>]) {
        let mut at = sector * SECTOR;
        for record in records {
            self.put(at, record);
            at += record.len();
        }
    }
}

/// One directory record. `name` is the raw stored bytes; `system_use` is the
/// Rock Ridge area that follows it.
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
    // Both-endian: little half then big half.
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

/// `.` and `..`, which every real directory begins with.
fn dot_records(here: u32, parent: u32) -> Vec<Vec<u8>> {
    vec![
        directory_record(here, u32::try_from(SECTOR).unwrap(), true, &[0], &[]),
        directory_record(parent, u32::try_from(SECTOR).unwrap(), true, &[1], &[]),
    ]
}

/// A valid image: one file and one folder holding one more file.
fn valid_image() -> Vec<u8> {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);

    let sub_sector = 19_u32;
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    root.push(directory_record(20, 11, false, b"README.TXT;1", &[]));
    root.push(directory_record(
        sub_sector,
        u32::try_from(SECTOR).unwrap(),
        true,
        b"SUB",
        &[],
    ));
    image.directory(ROOT_SECTOR, &root);

    let mut sub = dot_records(sub_sector, u32::try_from(ROOT_SECTOR).unwrap());
    sub.push(directory_record(21, 5, false, b"INNER.TXT;1", &[]));
    image.directory(sub_sector as usize, &sub);

    image.put(20 * SECTOR, b"hello world");
    image.put(21 * SECTOR, b"inner");
    image.bytes
}

fn names(path: &Path) -> Vec<String> {
    list_iso(path)
        .expect("listable")
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

#[test]
fn lists_files_and_folders_with_their_paths() {
    let path = write("valid.iso", &valid_image());
    assert_eq!(
        names(&path),
        vec![
            "README.TXT".to_string(),
            "SUB/".to_string(),
            "SUB/INNER.TXT".to_string(),
        ]
    );
}

#[test]
fn reports_each_file_size_and_where_its_bytes_are() {
    let path = write("sizes.iso", &valid_image());
    let entries = read_iso(&path).expect("listable");
    let readme = entries
        .iter()
        .find(|found| found.entry.name == "README.TXT")
        .expect("README is listed");
    assert_eq!(readme.entry.size, 11);
    assert_eq!(readme.extent.offset, 20 * SECTOR as u64);
    assert_eq!(readme.extent.length, 11);

    let folder = entries
        .iter()
        .find(|found| found.entry.name == "SUB/")
        .expect("SUB is listed");
    assert!(folder.entry.is_directory);
    assert_eq!(folder.entry.size, 0, "a folder has no size of its own");
}

/// `.` and `..` are structure. Listing them would put two rows in every
/// folder that navigate nowhere.
#[test]
fn the_dot_entries_are_not_listed() {
    let path = write("dots.iso", &valid_image());
    for name in names(&path) {
        assert!(name != "." && name != "..", "{name} should not be listed");
    }
}

/// Joliet holds the names whoever built the image meant people to see.
#[test]
fn a_joliet_tree_is_preferred_over_the_primary_one() {
    let mut image = Image::new(24);
    let primary_root = 18_u32;
    let joliet_root = 19_u32;
    image.primary(primary_root, u32::try_from(SECTOR).unwrap());
    image.joliet(17, joliet_root, u32::try_from(SECTOR).unwrap());
    image.terminator(18);

    let mut primary = dot_records(primary_root, primary_root);
    primary.push(directory_record(21, 4, false, b"SHOUTING.TXT;1", &[]));
    image.directory(primary_root as usize, &primary);

    // "Quiet.txt" as UCS-2 big endian.
    let joliet_name: Vec<u8> = "Quiet.txt"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    let mut joliet = dot_records(joliet_root, joliet_root);
    joliet.push(directory_record(21, 4, false, &joliet_name, &[]));
    image.directory(joliet_root as usize, &joliet);

    let path = write("joliet.iso", &image.bytes);
    assert_eq!(names(&path), vec!["Quiet.txt".to_string()]);
}

/// Rock Ridge `NM` is the real POSIX name and beats the uppercase one.
#[test]
fn a_rock_ridge_name_replaces_the_uppercase_one() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);

    // NM, length 4 + 1 flags + 9 name, version 1, flags 0.
    let mut nm = vec![b'N', b'M', 14, 1, 0];
    nm.extend_from_slice(b"readme.md");
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    root.push(directory_record(20, 4, false, b"README.MD;1", &nm));
    image.directory(ROOT_SECTOR, &root);

    let path = write("rockridge.iso", &image.bytes);
    assert_eq!(names(&path), vec!["readme.md".to_string()]);
}

/// A file too short to hold a descriptor is not an image.
#[test]
fn a_truncated_file_is_refused_rather_than_listed_as_empty() {
    let path = write("short.iso", &[0_u8; 1024]);
    assert!(list_iso(&path).is_err());
}

/// A file of the right size with no `CD001` is not an image either.
#[test]
fn a_file_with_no_volume_descriptor_is_refused() {
    let path = write("notiso.iso", &vec![0_u8; 24 * SECTOR]);
    assert!(list_iso(&path).is_err());
}

/// A record whose extent is past the end of the file must not be read.
///
/// The entry is still listed - it really is in the table - but with nothing
/// to read, so extraction has nothing to copy rather than a wild offset.
#[test]
fn a_record_pointing_outside_the_file_is_listed_with_no_bytes() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    // Sector 900 000 is far outside a 24-sector file.
    root.push(directory_record(900_000, 4096, false, b"GHOST.TXT;1", &[]));
    image.directory(ROOT_SECTOR, &root);

    let path = write("ghost.iso", &image.bytes);
    let entries = read_iso(&path).expect("the image itself is readable");
    let ghost = entries
        .iter()
        .find(|found| found.entry.name == "GHOST.TXT")
        .expect("listed");
    assert_eq!(ghost.extent.length, 0, "nothing to read outside the file");
}

/// A directory whose length runs past the end of the file is skipped, and the
/// rest of the image still lists.
#[test]
fn a_directory_longer_than_the_file_is_skipped() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    root.push(directory_record(20, 4, false, b"REAL.TXT;1", &[]));
    // A directory at sector 19 claiming 100 MB of records.
    root.push(directory_record(19, 100 * 1024 * 1024, true, b"HUGE", &[]));
    image.directory(ROOT_SECTOR, &root);

    let path = write("huge.iso", &image.bytes);
    let listed = names(&path);
    assert!(listed.contains(&"REAL.TXT".to_string()), "{listed:?}");
    // The folder is listed; what it claims to contain is not read.
    assert!(listed.contains(&"HUGE/".to_string()), "{listed:?}");
    assert!(
        listed.len() < 100,
        "a 100 MB claim produced {} entries",
        listed.len()
    );
}

/// A directory that points at itself is a cycle, and a cycle must end.
#[test]
fn a_directory_cycle_terminates() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    // A subdirectory whose extent is the root itself.
    root.push(directory_record(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
        true,
        b"LOOP",
        &[],
    ));
    image.directory(ROOT_SECTOR, &root);

    let path = write("loop.iso", &image.bytes);
    let listed = names(&path);
    assert_eq!(
        listed,
        vec!["LOOP/".to_string()],
        "the cycle produced {listed:?}"
    );
}

/// A name that climbs out of the extraction folder is flagged, not repaired.
#[test]
fn a_traversal_name_is_reported_as_unsafe() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);
    let mut root = dot_records(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(ROOT_SECTOR).unwrap(),
    );
    let mut nm = vec![b'N', b'M', 20, 1, 0];
    nm.extend_from_slice(b"../../etc/passwd");
    root.push(directory_record(20, 4, false, b"PASSWD;1", &nm));
    image.directory(ROOT_SECTOR, &root);

    let path = write("traversal.iso", &image.bytes);
    let entries = list_iso(&path).expect("listable");
    let escaping = entries
        .iter()
        .find(|entry| entry.name.contains(".."))
        .expect("the name is kept as stored, not rewritten");
    assert!(escaping.unsafe_name, "a climbing name must be flagged");
}

/// A record claiming a length that cannot be a record must not spin the walk.
#[test]
fn a_zero_length_record_does_not_loop() {
    let mut image = Image::new(24);
    image.primary(
        u32::try_from(ROOT_SECTOR).unwrap(),
        u32::try_from(SECTOR).unwrap(),
    );
    image.terminator(TERMINATOR_SECTOR);
    // A directory extent of nothing but zero bytes: every record length is 0.
    image.directory(ROOT_SECTOR, &[]);

    let path = write("zeros.iso", &image.bytes);
    assert_eq!(names(&path), Vec::<String>::new());
}
