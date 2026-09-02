//! What `diskutil` knows.
//!
//! `diskutil list -plist external physical` already answers most of the
//! question: *external* excludes the internal disk, and *physical* excludes
//! synthesised APFS containers, disk images and RAID sets. That is not taken on
//! trust — every disk it names is then asked about individually, and dropped
//! unless its own properties agree.
//!
//! The `-plist` form is used rather than the human-readable one because the
//! latter is laid out for reading, changes between releases, and puts the size
//! in a sentence.

#![allow(unreachable_pub)] // re-exported through `platform`.

use std::path::PathBuf;

use jtf_core::{Error, ErrorCode};

use super::run;
use crate::{Bus, Device, Volume};

/// Whether this platform can enumerate and write removable disks.
pub const fn is_supported() -> bool {
    true
}

/// Every external, physical, removable disk `diskutil` reports.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] if `diskutil` could not be run at all.
pub fn list() -> Result<Vec<Device>, Error> {
    let listing = run("diskutil", &["list", "-plist", "external", "physical"])?;
    let mounts = mounted_volumes();
    let mut devices = Vec::new();
    for id in whole_disks(&listing) {
        // One disk failing to describe itself drops that disk, not the list.
        if let Some(device) = describe(&id, &mounts) {
            devices.push(device);
        }
    }
    Ok(devices)
}

/// Unmount every volume of `device`, leaving the disk itself present.
///
/// `diskutil unmountDisk` does the whole disk in one step and, unlike
/// `umount`, tells the rest of the system it is going away — without it the
/// Finder remounts the volume mid-write.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] if a volume is in use and could not be
/// unmounted; the message names it.
pub fn unmount_volumes(device: &Device) -> Result<(), Error> {
    let node = whole_disk_node(device);
    let node = node.to_str().ok_or_else(|| {
        Error::new(ErrorCode::InvalidPath, "device node is not valid UTF-8")
    })?;
    run("diskutil", &["unmountDisk", node]).map(|_| ())
}

/// The buffered node (`/dev/disk4`) for a device, given either form.
///
/// Writing goes to the raw node `/dev/rdisk4`, which is far faster because it
/// bypasses the buffer cache; `diskutil` wants the buffered one.
fn whole_disk_node(device: &Device) -> PathBuf {
    let text = device.node.to_string_lossy();
    match text.strip_prefix("/dev/r") {
        Some(rest) => PathBuf::from(format!("/dev/{rest}")),
        None => device.node.clone(),
    }
}

/// The `WholeDisks` array from a `diskutil list -plist`.
fn whole_disks(plist: &str) -> Vec<String> {
    plist_array(plist, "WholeDisks")
}

/// Ask about one disk and decide whether it may be offered.
///
/// Returns `None` for anything this code cannot fully vouch for.
fn describe(id: &str, mounts: &[(String, PathBuf)]) -> Option<Device> {
    let node = format!("/dev/{id}");
    let info = run("diskutil", &["info", "-plist", &node]).ok()?;

    // Every one of these must say yes. A missing key is a no.
    if !plist_bool(&info, "Removable").unwrap_or(false)
        && !plist_bool(&info, "RemovableMedia").unwrap_or(false)
        && !plist_bool(&info, "RemovableMediaOrExternalDevice").unwrap_or(false)
    {
        return None;
    }
    if plist_bool(&info, "Internal").unwrap_or(true) {
        return None;
    }
    if plist_string(&info, "VirtualOrPhysical").as_deref() == Some("Virtual") {
        return None;
    }
    // The system disk is never external on macOS, so this cannot normally
    // trigger. It is here because "cannot normally" is not "cannot", and the
    // cost of the check is one string comparison.
    if plist_bool(&info, "SystemImage").unwrap_or(false)
        || plist_bool(&info, "OSInternal").unwrap_or(false)
    {
        return None;
    }

    let size = plist_integer(&info, "TotalSize").or_else(|| plist_integer(&info, "Size"))?;
    if size == 0 {
        // A card reader with no card in it reports a zero-size disk. Offering
        // it produces a write that fails on the first block.
        return None;
    }

    let model = plist_string(&info, "MediaName")
        .or_else(|| plist_string(&info, "IORegistryEntryName"))
        .unwrap_or_default();
    let bus = match plist_string(&info, "BusProtocol").as_deref() {
        Some("USB") => Bus::Usb,
        Some("Secure Digital" | "SD" | "MMC") => Bus::CardReader,
        _ => Bus::OtherExternal,
    };

    let prefix = format!("/dev/{id}");
    let volumes = mounts
        .iter()
        .filter(|(dev, _)| is_partition_of(dev, &prefix))
        .map(|(_, mount)| Volume {
            label: mount
                .file_name()
                .map(|n| n.to_string_lossy().into_owned()),
            mount_point: Some(mount.clone()),
        })
        .collect();

    Some(Device {
        // The raw node: writing through /dev/disk4 goes via the buffer cache
        // and is several times slower for no benefit.
        node: PathBuf::from(format!("/dev/r{id}")),
        model,
        size,
        bus,
        volumes,
    })
}

/// Whether `dev` is the whole disk `prefix` or one of its partitions.
///
/// `/dev/disk40s1` is not a partition of `/dev/disk4`, so the character after
/// the prefix has to be checked rather than the prefix alone.
fn is_partition_of(dev: &str, prefix: &str) -> bool {
    match dev.strip_prefix(prefix) {
        None => false,
        Some("") => true,
        Some(rest) => rest.starts_with('s'),
    }
}

/// Device-to-mount-point pairs, from `mount`.
///
/// An empty list on failure: not knowing where a disk is mounted makes the
/// warning less useful, not the write less safe.
fn mounted_volumes() -> Vec<(String, PathBuf)> {
    let Ok(text) = run("mount", &[]) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            // "/dev/disk4s1 on /Volumes/UNTITLED (msdos, local, nodev, ...)"
            let (device, rest) = line.split_once(" on ")?;
            let mount = rest.rsplit_once(" (").map_or(rest, |(before, _)| before);
            Some((device.to_string(), PathBuf::from(mount)))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// A very small plist reader.
//
// Enough for the flat dictionaries `diskutil` emits, and nothing more. A real
// plist parser is a dependency and a parser of untrusted input; this reads
// output the program just asked a system tool to produce, looking only for keys
// it names itself.
// ---------------------------------------------------------------------------

/// The text of the element immediately after `<key>name</key>`.
fn value_after<'a>(plist: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("<key>{key}</key>");
    let start = plist.find(&needle)? + needle.len();
    Some(plist.get(start..)?.trim_start())
}

fn plist_string(plist: &str, key: &str) -> Option<String> {
    let rest = value_after(plist, key)?;
    let inner = rest.strip_prefix("<string>")?;
    let end = inner.find("</string>")?;
    Some(unescape(&inner[..end]))
}

fn plist_integer(plist: &str, key: &str) -> Option<u64> {
    let rest = value_after(plist, key)?;
    let inner = rest.strip_prefix("<integer>")?;
    let end = inner.find("</integer>")?;
    inner[..end].trim().parse().ok()
}

fn plist_bool(plist: &str, key: &str) -> Option<bool> {
    let rest = value_after(plist, key)?;
    if rest.starts_with("<true/>") {
        Some(true)
    } else if rest.starts_with("<false/>") {
        Some(false)
    } else {
        None
    }
}

fn plist_array(plist: &str, key: &str) -> Vec<String> {
    let Some(rest) = value_after(plist, key) else {
        return Vec::new();
    };
    let Some(inner) = rest.strip_prefix("<array>") else {
        return Vec::new();
    };
    let Some(end) = inner.find("</array>") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = &inner[..end];
    while let Some(at) = cursor.find("<string>") {
        cursor = &cursor[at + "<string>".len()..];
        let Some(close) = cursor.find("</string>") else {
            break;
        };
        out.push(unescape(&cursor[..close]));
        cursor = &cursor[close..];
    }
    out
}

/// The five XML entities. `diskutil` emits a model name verbatim, and hardware
/// model names do contain ampersands.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>AllDisks</key><array><string>disk4</string><string>disk4s1</string></array>
  <key>WholeDisks</key><array><string>disk4</string><string>disk6</string></array>
</dict>
</plist>"#;

    const INFO: &str = r#"<plist version="1.0"><dict>
  <key>BusProtocol</key><string>USB</string>
  <key>Internal</key><false/>
  <key>MediaName</key><string>SanDisk &amp; Co Cruzer</string>
  <key>Removable</key><true/>
  <key>TotalSize</key><integer>15489564672</integer>
</dict></plist>"#;

    #[test]
    fn the_whole_disks_are_read_and_the_partitions_are_not() {
        assert_eq!(whole_disks(LIST), vec!["disk4", "disk6"]);
    }

    #[test]
    fn a_flat_dictionary_gives_up_its_strings_integers_and_booleans() {
        assert_eq!(plist_string(INFO, "BusProtocol").as_deref(), Some("USB"));
        assert_eq!(plist_integer(INFO, "TotalSize"), Some(15_489_564_672));
        assert_eq!(plist_bool(INFO, "Internal"), Some(false));
        assert_eq!(plist_bool(INFO, "Removable"), Some(true));
    }

    #[test]
    fn an_ampersand_in_a_model_name_survives() {
        assert_eq!(
            plist_string(INFO, "MediaName").as_deref(),
            Some("SanDisk & Co Cruzer")
        );
    }

    #[test]
    fn a_key_that_is_not_there_reads_as_absent_rather_than_as_false() {
        // The difference matters: `Internal` missing must not be read as
        // "not internal".
        assert_eq!(plist_bool(INFO, "OSInternal"), None);
        assert_eq!(plist_integer(INFO, "Nope"), None);
        assert_eq!(plist_string(INFO, "Nope"), None);
    }

    #[test]
    fn a_truncated_plist_yields_nothing_rather_than_a_wrong_answer() {
        let cut = &INFO[..INFO.find("15489564672").unwrap() + 4];
        assert_eq!(plist_integer(cut, "TotalSize"), None);
        assert!(plist_array(cut, "WholeDisks").is_empty());
    }

    #[test]
    fn a_partition_belongs_to_its_disk_and_a_longer_number_does_not() {
        assert!(is_partition_of("/dev/disk4", "/dev/disk4"));
        assert!(is_partition_of("/dev/disk4s1", "/dev/disk4"));
        // disk40 is a different disk that happens to share a prefix.
        assert!(!is_partition_of("/dev/disk40", "/dev/disk4"));
        assert!(!is_partition_of("/dev/disk40s1", "/dev/disk4"));
    }

    #[test]
    fn the_raw_node_is_turned_back_into_the_one_diskutil_wants() {
        let device = Device {
            node: PathBuf::from("/dev/rdisk4"),
            model: String::new(),
            size: 1,
            bus: Bus::Usb,
            volumes: Vec::new(),
        };
        assert_eq!(whole_disk_node(&device), PathBuf::from("/dev/disk4"));
    }
}
