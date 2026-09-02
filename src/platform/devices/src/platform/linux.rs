//! What `/sys/block` knows.
//!
//! No tool to shell out to here: the kernel already publishes everything
//! needed, as files. That is better than parsing `lsblk`, which may not be
//! installed, and much better than guessing from device names — `/dev/sdb` is
//! a removable stick on one machine and the second internal disk on the next.
//!
//! The disk holding `/` is found from `/proc/mounts` and excluded by name as
//! well as by the removable flag, because a machine booted from a USB stick has
//! a root filesystem on a disk that is genuinely removable.

#![allow(unreachable_pub)] // re-exported through `platform`.

use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};

use super::run;
use crate::{Bus, Device, Volume};

/// Whether this platform can enumerate and write removable disks.
pub const fn is_supported() -> bool {
    true
}

/// Every removable block device that is not carrying the running system.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] if `/sys/block` could not be read at all,
/// which means something is very wrong with the machine.
pub fn list() -> Result<Vec<Device>, Error> {
    let entries = std::fs::read_dir("/sys/block").map_err(|e| {
        Error::new(
            ErrorCode::ProviderFailed,
            format!("cannot read /sys/block: {e}"),
        )
    })?;

    let mounts = mounted_volumes();
    let system = system_disks(&mounts);
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if system.iter().any(|d| *d == name) {
            continue;
        }
        if let Some(device) = describe(&name, &mounts) {
            devices.push(device);
        }
    }
    Ok(devices)
}

/// Unmount every mounted volume of `device`.
///
/// `udisksctl` where it exists, because it unmounts as the desktop user and
/// tells the desktop the volume is gone; plain `umount` otherwise, which needs
/// the mount to be the user's own or the caller to be privileged.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] naming the volume that would not unmount.
pub fn unmount_volumes(device: &Device) -> Result<(), Error> {
    let name = disk_name(&device.node)?;
    for (node, _) in mounted_volumes() {
        if !is_partition_of(&node, &format!("/dev/{name}")) {
            continue;
        }
        let unmounted = run("udisksctl", &["unmount", "-b", &node])
            .or_else(|_| run("umount", &[node.as_str()]));
        unmounted.map_err(|e| {
            Error::new(
                ErrorCode::ProviderFailed,
                format!("{node} is still mounted: {e}"),
            )
        })?;
    }
    Ok(())
}

/// The kernel's name for a disk, from a node path.
fn disk_name(node: &Path) -> Result<String, Error> {
    node.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| Error::new(ErrorCode::InvalidPath, "device node has no name"))
}

/// Decide whether one `/sys/block` entry may be offered.
fn describe(name: &str, mounts: &[(String, PathBuf)]) -> Option<Device> {
    // Virtual block devices are not disks and are never a write target. Named
    // rather than inferred, so a new one does not appear in the list by
    // default.
    const VIRTUAL: [&str; 7] = ["loop", "ram", "zram", "dm-", "md", "nbd", "sr"];
    if VIRTUAL.iter().any(|p| name.starts_with(p)) {
        return None;
    }

    let base = PathBuf::from("/sys/block").join(name);
    let removable = read_trimmed(&base.join("removable"))? == "1";
    let bus = bus_of(&base);
    // Removable *or* on a removable bus: a USB SSD reports removable=0 because
    // the media cannot be taken out of the enclosure, and is still a stick as
    // far as this program is concerned. A disk that is neither is internal.
    let bus = match (removable, bus) {
        (_, Some(bus)) => bus,
        (true, None) => Bus::OtherExternal,
        (false, None) => return None,
    };

    // `size` is in 512-byte sectors regardless of the disk's own sector size.
    let sectors: u64 = read_trimmed(&base.join("size"))?.parse().ok()?;
    let size = sectors.checked_mul(512)?;
    if size == 0 {
        // An empty card reader. Offering it produces a write that fails on the
        // first block.
        return None;
    }

    let vendor = read_trimmed(&base.join("device/vendor")).unwrap_or_default();
    let model = read_trimmed(&base.join("device/model")).unwrap_or_default();
    let label = format!("{} {}", vendor.trim(), model.trim())
        .trim()
        .to_string();

    let prefix = format!("/dev/{name}");
    let volumes = mounts
        .iter()
        .filter(|(node, _)| is_partition_of(node, &prefix))
        .map(|(_, mount)| Volume {
            label: mount.file_name().map(|n| n.to_string_lossy().into_owned()),
            mount_point: Some(mount.clone()),
        })
        .collect();

    Some(Device {
        node: PathBuf::from(prefix),
        model: label,
        size,
        bus,
        volumes,
    })
}

/// The bus a disk hangs off, where it is one this program treats as external.
///
/// Read from the `device` symlink's target, which spells out the path through
/// the device tree: a USB disk's contains `/usb`.
fn bus_of(base: &Path) -> Option<Bus> {
    let link = std::fs::read_link(base.join("device")).ok()?;
    let text = link.to_string_lossy();
    if text.contains("/usb") {
        Some(Bus::Usb)
    } else if text.contains("/mmc") {
        Some(Bus::CardReader)
    } else if text.contains("/firewire") || text.contains("/thunderbolt") {
        Some(Bus::OtherExternal)
    } else {
        None
    }
}

/// The disks carrying a filesystem the running system needs.
///
/// `/` and `/boot` at minimum. A machine booted from a USB stick has a root
/// filesystem on a removable disk, and every other check here would wave it
/// through.
fn system_disks(mounts: &[(String, PathBuf)]) -> Vec<String> {
    const CRITICAL: [&str; 4] = ["/", "/boot", "/boot/efi", "/usr"];
    mounts
        .iter()
        .filter(|(_, mount)| CRITICAL.iter().any(|c| mount == Path::new(c)))
        .filter_map(|(node, _)| whole_disk_of(node))
        .collect()
}

/// The kernel name of the disk a partition node belongs to.
///
/// Asked of the kernel rather than by stripping digits, because `nvme0n1p2`
/// belongs to `nvme0n1` and `sda2` belongs to `sda`, and no string rule covers
/// both.
fn whole_disk_of(node: &str) -> Option<String> {
    let name = node.strip_prefix("/dev/")?;
    // /sys/class/block/<partition>/.. is the disk, for a partition; for a whole
    // disk the entry is directly under /sys/block.
    let partition = PathBuf::from("/sys/class/block").join(name);
    if partition.join("partition").exists() {
        let parent = std::fs::canonicalize(partition.join("..")).ok()?;
        return parent
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
    }
    Path::new("/sys/block")
        .join(name)
        .exists()
        .then(|| name.to_string())
}

/// Whether `node` is the whole disk `prefix` or one of its partitions.
///
/// `/dev/sdb1` belongs to `/dev/sdb`; `/dev/sdba` does not. On NVMe the
/// separator is `p`, and `/dev/nvme0n11` is a different namespace from
/// `/dev/nvme0n1`, so a bare digit only counts after a `sd`-style name.
fn is_partition_of(node: &str, prefix: &str) -> bool {
    match node.strip_prefix(prefix) {
        None => false,
        Some("") => true,
        Some(rest) => {
            if prefix.contains("nvme") || prefix.contains("mmcblk") {
                rest.strip_prefix('p').is_some_and(|n| all_digits(n))
            } else {
                all_digits(rest)
            }
        }
    }
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// Device-to-mount-point pairs from `/proc/mounts`.
fn mounted_volumes() -> Vec<(String, PathBuf)> {
    let Ok(text) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    parse_mounts(&text)
}

/// Split `/proc/mounts`, undoing the octal escaping the kernel applies.
fn parse_mounts(text: &str) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let node = fields.next()?;
            let mount = fields.next()?;
            node.starts_with("/dev/")
                .then(|| (node.to_string(), PathBuf::from(unescape_mount(mount))))
        })
        .collect()
}

/// `/proc/mounts` escapes space, tab, newline and backslash as `\040` and
/// friends. A volume called "My Stick" is mounted at `/media/jason/My\040Stick`
/// and comparing that against a real path matches nothing.
fn unescape_mount(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let octal = &text[i + 1..i + 4];
            if let Ok(byte) = u8::from_str_radix(octal, 8) {
                out.push(byte as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_partition_belongs_to_its_disk_and_a_similar_name_does_not() {
        assert!(is_partition_of("/dev/sdb", "/dev/sdb"));
        assert!(is_partition_of("/dev/sdb1", "/dev/sdb"));
        assert!(is_partition_of("/dev/sdb12", "/dev/sdb"));
        // A different disk that happens to share a prefix.
        assert!(!is_partition_of("/dev/sdba", "/dev/sdb"));
        assert!(!is_partition_of("/dev/sdc1", "/dev/sdb"));
    }

    #[test]
    fn nvme_and_mmc_partitions_need_the_p() {
        assert!(is_partition_of("/dev/nvme0n1p2", "/dev/nvme0n1"));
        assert!(is_partition_of("/dev/mmcblk0p1", "/dev/mmcblk0"));
        // nvme0n11 is a second namespace, not a partition of the first.
        assert!(!is_partition_of("/dev/nvme0n11", "/dev/nvme0n1"));
    }

    #[test]
    fn proc_mounts_is_split_into_device_and_mount_point() {
        let text = "\
sysfs /sys sysfs rw,nosuid 0 0
/dev/sda2 / ext4 rw,relatime 0 0
/dev/sdb1 /media/jason/STICK vfat rw,nosuid 0 0
";
        let mounts = parse_mounts(text);
        assert_eq!(mounts.len(), 2, "only real devices, not sysfs");
        assert_eq!(mounts[0].0, "/dev/sda2");
        assert_eq!(mounts[0].1, Path::new("/"));
        assert_eq!(mounts[1].1, Path::new("/media/jason/STICK"));
    }

    #[test]
    fn a_mount_point_with_a_space_in_it_comes_back_with_the_space() {
        // The kernel writes /media/jason/My\040Stick, and comparing that
        // against a real path matches nothing at all.
        let mounts = parse_mounts("/dev/sdb1 /media/jason/My\\040Stick vfat rw 0 0\n");
        assert_eq!(mounts[0].1, Path::new("/media/jason/My Stick"));
    }

    #[test]
    fn a_backslash_that_is_not_an_escape_is_left_alone() {
        assert_eq!(unescape_mount("/media/a\\b"), "/media/a\\b");
    }

    #[test]
    fn the_disk_holding_root_is_named_as_a_system_disk() {
        // Only meaningful on a real Linux machine; elsewhere the sysfs lookup
        // finds nothing and the list is empty, which is the safe direction.
        let mounts = mounted_volumes();
        if mounts.iter().any(|(_, m)| m == Path::new("/")) {
            let system = system_disks(&mounts);
            assert!(
                !system.is_empty(),
                "root is mounted but no system disk was identified"
            );
        }
    }

    #[test]
    fn no_disk_carrying_the_system_is_ever_offered() {
        let Ok(devices) = list() else { return };
        let mounts = mounted_volumes();
        let system = system_disks(&mounts);
        for device in &devices {
            let name = disk_name(&device.node).unwrap();
            assert!(
                !system.contains(&name),
                "{name} carries the running system and was offered anyway"
            );
        }
    }
}
