//! What enumeration says about the machine actually running the tests.
//!
//! Everything else in this crate is checked against captured output, which
//! proves the parsing and not the question. These check the answer against the
//! machine: whatever disks this computer has, the ones it is running from must
//! not be among the ones offered.
//!
//! They pass trivially on a machine with nothing plugged in, which is most
//! machines and every CI runner. That is the point — the assertion that must
//! never fire is the one about the system disk, and it is worth having on every
//! developer's machine even though it usually has nothing to say.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use jtf_platform_devices as devices;

/// Every mount point on this machine, paired with its device node.
fn mounts() -> Vec<(String, PathBuf)> {
    #[cfg(target_os = "linux")]
    {
        let Ok(text) = std::fs::read_to_string("/proc/mounts") else {
            return Vec::new();
        };
        return text
            .lines()
            .filter_map(|line| {
                let mut f = line.split_whitespace();
                let node = f.next()?;
                let mount = f.next()?;
                node.starts_with("/dev/")
                    .then(|| (node.to_string(), PathBuf::from(mount)))
            })
            .collect();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let Ok(out) = std::process::Command::new("mount").output() else {
            return Vec::new();
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let (node, rest) = line.split_once(" on ")?;
                let mount = rest.rsplit_once(" (").map_or(rest, |(b, _)| b);
                Some((node.to_string(), PathBuf::from(mount)))
            })
            .collect()
    }
}

#[test]
fn the_disk_this_machine_booted_from_is_never_offered() {
    // The one assertion this whole crate exists to make true.
    let Ok(offered) = devices::list() else {
        return; // A platform with no enumeration offers nothing at all.
    };
    let roots: Vec<String> = mounts()
        .into_iter()
        .filter(|(_, mount)| {
            mount == Path::new("/") || mount == Path::new("/boot") || mount == Path::new("/boot/efi")
        })
        .map(|(node, _)| node)
        .collect();

    for device in &offered {
        let node = device.node.to_string_lossy().replace("/dev/r", "/dev/");
        for root in &roots {
            assert!(
                !root.starts_with(node.as_str()),
                "{} carries {root}, and was offered as a write target",
                device.node.display()
            );
        }
    }
}

#[test]
fn nothing_offered_has_a_zero_size() {
    // A card reader with no card is a disk of size zero. Writing to it fails
    // on the first block, after the user has confirmed destroying it.
    let Ok(offered) = devices::list() else { return };
    for device in &offered {
        assert!(device.size > 0, "{} has no size", device.node.display());
    }
}

#[test]
fn everything_offered_has_a_node_that_exists() {
    let Ok(offered) = devices::list() else { return };
    for device in &offered {
        // On Windows the node is a device namespace path, which does not
        // answer to `exists`.
        if cfg!(target_os = "windows") {
            assert!(
                device.node.to_string_lossy().contains("PhysicalDrive"),
                "{} is not a physical drive path",
                device.node.display()
            );
        } else {
            assert!(
                device.node.exists(),
                "{} was offered and is not there",
                device.node.display()
            );
        }
    }
}

#[test]
fn asking_twice_gives_the_same_answer_in_the_same_order() {
    // The list is shown in a dialog that refreshes. If the order moved between
    // refreshes, a cursor sitting on one disk would end up on another.
    let (Ok(first), Ok(second)) = (devices::list(), devices::list()) else {
        return;
    };
    assert_eq!(first, second);
}

#[test]
fn this_platform_says_whether_it_can_do_this_at_all() {
    // Rather than reporting an empty list, which reads as "no disks plugged
    // in" and is a different thing.
    let supported = devices::is_supported();
    if !supported {
        assert!(devices::list().is_err());
    }
}
