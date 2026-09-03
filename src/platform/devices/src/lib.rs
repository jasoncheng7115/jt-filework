//! The removable disks a person may be offered as a write target.
//!
//! Writing an image to a disk destroys everything on it, with no undo and no
//! trash. That makes the *list* the safety mechanism, not the confirmation
//! dialog: by the time someone is reading a dialog they have already decided,
//! and the only thing standing between a routine action and a wiped backup
//! drive is whether the wrong disk was ever offered.
//!
//! So the list is a whitelist. A disk appears only if this code can positively
//! establish that it is removable, external, and not carrying the running
//! system. Every other case — an internal disk, a disk whose properties could
//! not be read, a platform whose enumeration is not implemented — produces no
//! entry at all. There is no path through this module where "I could not tell"
//! results in a disk being shown.
//!
//! In its own crate because asking what disks exist is different on each of the
//! three platforms and nowhere else needs to know that (`AGENTS.md` §5).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::{Path, PathBuf};

use jtf_core::{Error, ErrorCode};
use serde::{Deserialize, Serialize};

mod platform;
mod writer;

pub use platform::{is_supported, unmount_volumes};
pub use writer::{needs_elevation, open, open_for_read, Sink};

/// How a disk is attached.
///
/// Used for display and for the refusal reason, never as the sole basis for
/// letting a disk through: a USB enclosure holding someone's only backup is
/// removable, external and precious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bus {
    /// USB, in any of its generations.
    Usb,
    /// An SD or other memory card reader.
    CardReader,
    /// Thunderbolt, `FireWire`, or another external bus.
    OtherExternal,
}

impl Bus {
    /// Localization key for the bus name.
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Usb => "device.bus.usb",
            Self::CardReader => "device.bus.card_reader",
            Self::OtherExternal => "device.bus.other_external",
        }
    }
}

/// A volume currently mounted from a disk.
///
/// Shown to the user before they commit, because "8 GB removable disk" is not
/// enough to tell two sticks apart and "8 GB removable disk holding TAX-2025"
/// is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    /// What the volume calls itself, where it has a name.
    pub label: Option<String>,
    /// Where it is mounted, where it is mounted.
    pub mount_point: Option<PathBuf>,
}

/// A disk that may be offered as a write target.
///
/// Constructing one is not a claim that writing to it is a good idea. It is a
/// claim that this code established the disk is removable, external and not the
/// running system's own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// The path the platform writes through: `/dev/rdisk4`, `/dev/sdb`,
    /// `\\.\PhysicalDrive2`.
    pub node: PathBuf,
    /// What the hardware calls itself, for telling two disks apart.
    pub model: String,
    /// Capacity in bytes.
    pub size: u64,
    /// How it is attached.
    pub bus: Bus,
    /// The volumes mounted from it right now.
    pub volumes: Vec<Volume>,
}

impl Device {
    /// A name to show in a list: the model, or the node if there is no model.
    pub fn display_name(&self) -> &str {
        if self.model.trim().is_empty() {
            self.node.to_str().unwrap_or("?")
        } else {
            self.model.trim()
        }
    }

    /// Whether an image of `bytes` fits.
    pub const fn fits(&self, bytes: u64) -> bool {
        bytes <= self.size
    }
}

/// Why a disk cannot be written to.
///
/// A reason, not a boolean, because the user is owed the reason. "That disk is
/// not offered" invites a fight with the program; "that disk holds the running
/// system" ends the question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The disk holds the running operating system.
    SystemDisk,
    /// The disk is built in, not removable.
    Internal,
    /// The disk holds the image that would be written to it.
    HoldsTheSource,
    /// The image is larger than the disk.
    TooSmall {
        /// What the image needs.
        needed: u64,
        /// What the disk has.
        available: u64,
    },
    /// The properties needed to judge the disk could not be read.
    ///
    /// Deliberately a refusal rather than a shrug: an unreadable disk is
    /// exactly the case where guessing is worst.
    Unknown,
}

impl Refusal {
    /// Localization key for the reason shown to the user.
    pub const fn message_key(self_: &Self) -> &'static str {
        match *self_ {
            Self::SystemDisk => "device.refuse.system_disk",
            Self::Internal => "device.refuse.internal",
            Self::HoldsTheSource => "device.refuse.holds_the_source",
            Self::TooSmall { .. } => "device.refuse.too_small",
            Self::Unknown => "device.refuse.unknown",
        }
    }
}

/// Every removable, external, non-system disk this machine can see.
///
/// An empty list is a normal answer — most machines have no removable disk
/// plugged in — and is not an error.
///
/// # Errors
///
/// [`ErrorCode::Unsupported`] where enumeration is not implemented for the
/// platform, and whatever the platform reported if the query itself failed.
/// Never an error merely because a single disk could not be read: that disk is
/// dropped from the list, which is the safe direction.
pub fn list() -> Result<Vec<Device>, Error> {
    let mut found = platform::list()?;
    // A stable order, so the list does not reshuffle under someone's cursor
    // between one refresh and the next.
    found.sort_by(|a, b| a.node.cmp(&b.node));
    Ok(found)
}

/// Whether `image` may be written to `device`, and why not when it may not.
///
/// The size and source checks live here rather than in the platform code
/// because they are the same question on every platform, and because they are
/// worth testing without a disk plugged in.
///
/// # Errors
///
/// The [`Refusal`] that applies, which is the reason to show the user.
pub fn check(device: &Device, image: &Path, image_size: u64) -> Result<(), Refusal> {
    if image_size == 0 {
        // Not a size refusal: a zero-byte image is not an image.
        return Err(Refusal::Unknown);
    }
    if !device.fits(image_size) {
        return Err(Refusal::TooSmall {
            needed: image_size,
            available: device.size,
        });
    }
    if holds(device, image) {
        return Err(Refusal::HoldsTheSource);
    }
    Ok(())
}

/// Whether `path` lives on one of `device`'s mounted volumes.
///
/// Writing an image to the disk it is being read from destroys the image
/// halfway through writing it, and the failure looks like a hardware fault
/// rather than the mistake it was.
fn holds(device: &Device, path: &Path) -> bool {
    // Both sides resolved the same way, because comparing a resolved path
    // against an unresolved one answers "different disk" for two names of the
    // same place. On macOS `/var` is a link to `/private/var`, so an image
    // under a resolved `/private/var/...` was held against a mount point
    // still written `/var/...` and the disk holding the source was offered
    // for writing.
    let absolute = resolve(path);
    device.volumes.iter().any(|volume| {
        volume
            .mount_point
            .as_ref()
            .is_some_and(|mount| under(&absolute, &resolve(mount)))
    })
}

/// A path in the form the filesystem itself would give it, as far as it can
/// be known: links followed if it exists, otherwise made absolute.
fn resolve(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_or_else(|_| path.to_path_buf(), |d| d.join(path))
        }
    })
}

/// Whether `path` is `root` or inside it, compared component by component.
///
/// Not `starts_with` on the string form: `/media/usb2` starts with `/media/usb`
/// and is a different disk.
fn under(path: &Path, root: &Path) -> bool {
    // The filesystem root mounts everything, and treating it as "holds the
    // source" would refuse every disk on a machine with no separate mounts.
    if root.parent().is_none() {
        return false;
    }
    path.starts_with(root)
}

/// The error for a platform that cannot do this.
#[cfg_attr(
    any(target_os = "macos", target_os = "linux", target_os = "windows"),
    allow(dead_code)
)]
fn unsupported(what: &str) -> Error {
    Error::new(
        ErrorCode::Unsupported,
        format!("{what} is not implemented on this platform"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that really exists, so `canonicalize` succeeds and the
    /// comparison is between two paths of the same shape.
    ///
    /// Hard-coded `/Volumes/UNTITLED` worked on macOS and quietly did not on
    /// Windows: a path with a root but no drive letter is not absolute there,
    /// so the fallback joined it under the working directory and the
    /// containment check then compared `C:\...\Volumes\UNTITLED` against
    /// `/Volumes/UNTITLED` and answered "different disk". The test that was
    /// meant to prove the source is protected proved nothing at all.
    fn mount(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jtf-devices-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mount point");
        dir
    }

    fn stick(size: u64) -> Device {
        Device {
            node: PathBuf::from("/dev/rdisk4"),
            model: "SanDisk Cruzer".into(),
            size,
            bus: Bus::Usb,
            volumes: vec![Volume {
                label: Some("UNTITLED".into()),
                mount_point: Some(mount("UNTITLED")),
            }],
        }
    }

    #[test]
    fn an_image_larger_than_the_disk_is_refused_with_both_numbers() {
        let device = stick(1_000);
        let err = check(&device, Path::new("/tmp/x.iso"), 2_000).unwrap_err();
        assert_eq!(
            err,
            Refusal::TooSmall {
                needed: 2_000,
                available: 1_000
            }
        );
    }

    #[test]
    fn an_image_exactly_the_size_of_the_disk_fits() {
        // The boundary matters: an image written to a disk of exactly its size
        // is the normal case for a disk image taken from that same model.
        assert!(check(&stick(1_000), Path::new("/tmp/x.iso"), 1_000).is_ok());
    }

    #[test]
    fn an_empty_image_is_not_an_image() {
        assert_eq!(
            check(&stick(1_000), Path::new("/tmp/x.iso"), 0).unwrap_err(),
            Refusal::Unknown
        );
    }

    #[test]
    fn a_disk_is_refused_when_the_image_is_sitting_on_it() {
        // Reading the source from the disk being overwritten destroys the
        // source partway through, and looks like a hardware fault.
        let device = stick(8_000_000_000);
        let source = mount("UNTITLED").join("ubuntu.iso");
        std::fs::write(&source, b"x").expect("image");
        assert_eq!(
            check(&device, &source, 1_000).unwrap_err(),
            Refusal::HoldsTheSource
        );
        let _ = std::fs::remove_file(&source);
    }

    #[test]
    fn a_mount_point_that_is_only_a_string_prefix_is_a_different_disk() {
        // `USB2` starts with `USB` as text and is a different disk. Built from
        // real directories so the comparison is between paths of the same
        // shape on every platform - as strings these both passed on Windows
        // for a reason that had nothing to do with the rule being tested.
        let mut device = stick(8_000_000_000);
        device.volumes = vec![Volume {
            label: None,
            mount_point: Some(mount("USB")),
        }];
        let elsewhere = mount("USB2").join("x.iso");
        std::fs::write(&elsewhere, b"x").expect("image");
        assert!(check(&device, &elsewhere, 1_000).is_ok());
        let _ = std::fs::remove_file(&elsewhere);
    }

    #[test]
    fn the_filesystem_root_does_not_count_as_holding_everything() {
        // A disk mounted at / would otherwise refuse every image on the
        // machine, since every path is under /.
        let mut device = stick(8_000_000_000);
        device.volumes = vec![Volume {
            label: None,
            mount_point: Some(PathBuf::from("/")),
        }];
        assert!(check(&device, Path::new("/home/jason/x.iso"), 1_000).is_ok());
    }

    #[test]
    fn a_disk_with_no_model_still_has_something_to_show() {
        let mut device = stick(1_000);
        device.model = "   ".into();
        assert_eq!(device.display_name(), "/dev/rdisk4");
    }

    #[test]
    fn listing_never_reports_an_error_for_a_single_unreadable_disk() {
        // Whatever this machine has, enumeration either works or says the
        // platform cannot do it. It must not fail because one disk was odd.
        match list() {
            Ok(_) => {}
            Err(e) => assert_eq!(e.code(), ErrorCode::Unsupported, "{e}"),
        }
    }
}
