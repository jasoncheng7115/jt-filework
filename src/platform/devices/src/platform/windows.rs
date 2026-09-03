//! What `Get-Disk` knows.
//!
//! Windows is the one platform that answers the safety question directly:
//! `IsBoot` and `IsSystem` are properties of the disk, so there is no need to
//! work backwards from which volume the system booted off. `BusType` supplies
//! the rest.
//!
//! Output is asked for as JSON. The alternative is the table form, which is
//! laid out for reading, is localised, and truncates the model name with an
//! ellipsis at whatever width the console happens to be.

#![allow(unreachable_pub)] // re-exported through `platform`.

use std::path::PathBuf;

use jtf_core::{Error, ErrorCode};

use super::run;
use crate::{Bus, Device, Volume};

/// Whether this platform can enumerate and write removable disks.
pub const fn is_supported() -> bool {
    true
}

/// Every removable, non-boot, non-system disk `Get-Disk` reports.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] if PowerShell could not be run.
pub fn list() -> Result<Vec<Device>, Error> {
    // Forced to an array so a machine with exactly one removable disk gives a
    // one-element array rather than a bare object, which would otherwise need
    // a second parse path that nobody would ever exercise.
    //
    // Through `@(...)` and `-InputObject`, not `-AsArray`: that switch
    // arrived in PowerShell 6, and `powershell.exe` on a stock Windows is
    // Windows PowerShell 5.1. It did not degrade - the whole command failed
    // with "cannot find a parameter named AsArray", so enumeration returned
    // an error and the writer offered no disks at all on an ordinary machine.
    let script = "$d = @(Get-Disk | Select-Object Number,FriendlyName,Size,BusType,IsBoot,\
                  IsSystem,IsOffline,OperationalStatus); \
                  ConvertTo-Json -InputObject $d -Compress";
    let json = run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )?;
    let volumes = volumes_by_disk();
    Ok(parse_disks(&json)
        .into_iter()
        .filter_map(|disk| disk.into_device(&volumes))
        .collect())
}

/// Dismount every volume of `device` so the write is not fighting the
/// filesystem driver for the same sectors.
///
/// # Errors
///
/// [`ErrorCode::ProviderFailed`] if a volume could not be dismounted.
pub fn unmount_volumes(device: &Device) -> Result<(), Error> {
    let number = disk_number(device)?;
    // Taking the disk offline is what the built-in tools do before a raw
    // write; it dismounts every volume and stops the disk being remounted the
    // moment a partition table appears.
    let script = format!(
        "Set-Disk -Number {number} -IsOffline $true -ErrorAction Stop; \
         Set-Disk -Number {number} -IsReadOnly $false -ErrorAction SilentlyContinue"
    );
    run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )
    .map(|_| ())
}

/// The disk number behind a `\\.\PhysicalDriveN` node.
fn disk_number(device: &Device) -> Result<u32, Error> {
    device
        .node
        .to_string_lossy()
        .rsplit("PhysicalDrive")
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            Error::new(
                ErrorCode::InvalidPath,
                format!("{} is not a physical drive node", device.node.display()),
            )
        })
}

/// One disk as `Get-Disk` describes it.
struct Disk {
    number: u32,
    name: String,
    size: u64,
    bus: String,
    boot: bool,
    system: bool,
    offline: bool,
}

impl Disk {
    /// Turn a described disk into an offerable device, or into nothing.
    fn into_device(self, volumes: &[(u32, Volume)]) -> Option<Device> {
        // Either flag alone is disqualifying, and both are trusted over
        // anything the bus type says: a removable disk that the system booted
        // from is still the disk the system booted from.
        if self.boot || self.system {
            return None;
        }
        let bus = match self.bus.as_str() {
            "USB" => Bus::Usb,
            "SD" | "MMC" => Bus::CardReader,
            "1394" | "Fibre Channel" | "iSCSI" => Bus::OtherExternal,
            // Everything else - SATA, NVMe, RAID, SAS, an empty string from a
            // machine whose Get-Disk lacks the property - is not offered.
            _ => return None,
        };
        if self.size == 0 {
            return None;
        }
        Some(Device {
            node: PathBuf::from(format!(r"\\.\PhysicalDrive{}", self.number)),
            model: self.name,
            size: self.size,
            bus,
            volumes: volumes
                .iter()
                .filter(|(n, _)| *n == self.number)
                // An offline disk has no mounted volumes, whatever the volume
                // query said a moment ago.
                .filter(|_| !self.offline)
                .map(|(_, v)| v.clone())
                .collect(),
        })
    }
}

/// Pull the fields this code needs out of `ConvertTo-Json` output.
///
/// Hand-written rather than `serde_json` because the shape is fixed and known:
/// a flat array of flat objects, produced by a command this program wrote
/// itself. A missing or unparseable field drops that disk.
fn parse_disks(json: &str) -> Vec<Disk> {
    json_objects(json)
        .into_iter()
        .filter_map(|object| {
            Some(Disk {
                number: json_number(&object, "Number")?.try_into().ok()?,
                name: json_string(&object, "FriendlyName").unwrap_or_default(),
                size: json_number(&object, "Size")?,
                bus: json_string(&object, "BusType").unwrap_or_default(),
                // A missing safety flag reads as "yes, it is the boot disk".
                boot: json_bool(&object, "IsBoot").unwrap_or(true),
                system: json_bool(&object, "IsSystem").unwrap_or(true),
                offline: json_bool(&object, "IsOffline").unwrap_or(false),
            })
        })
        .collect()
}

/// The top-level objects of a flat JSON array, as raw text.
fn json_objects(json: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, ch) in json.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    objects.push(json[start..=i].to_string());
                }
            }
            _ => {}
        }
    }
    objects
}

/// The raw text of `"key":` value, up to the next comma or closing brace.
fn json_value<'a>(object: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = object.find(&needle)? + needle.len();
    let rest = object.get(start..)?.trim_start();
    Some(rest)
}

fn json_string(object: &str, key: &str) -> Option<String> {
    let rest = json_value(object, key)?;
    let inner = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            out.push(match ch {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn json_number(object: &str, key: &str) -> Option<u64> {
    let rest = json_value(object, key)?;
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest.get(..end)?.parse().ok()
}

fn json_bool(object: &str, key: &str) -> Option<bool> {
    let rest = json_value(object, key)?;
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// Mounted volumes, paired with the disk number they sit on.
///
/// Best effort: not knowing a stick's drive letter makes the warning less
/// useful, not the write less safe.
fn volumes_by_disk() -> Vec<(u32, Volume)> {
    // Same array-forcing as `list`, for the same reason: one partition would
    // otherwise come back as a bare object.
    let script = "$p = @(Get-Partition | ForEach-Object { \
                  $v = $_ | Get-Volume -ErrorAction SilentlyContinue; \
                  [pscustomobject]@{ Disk = $_.DiskNumber; \
                  Letter = $_.DriveLetter; Label = $v.FileSystemLabel } }); \
                  ConvertTo-Json -InputObject $p -Compress";
    let Ok(json) = run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    ) else {
        return Vec::new();
    };
    json_objects(&json)
        .into_iter()
        .filter_map(|object| {
            let disk = json_number(&object, "Disk")?.try_into().ok()?;
            let letter = json_string(&object, "Letter").filter(|l| !l.is_empty());
            let label = json_string(&object, "Label").filter(|l| !l.is_empty());
            Some((
                disk,
                Volume {
                    label,
                    mount_point: letter.map(|l| PathBuf::from(format!("{l}:\\"))),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISKS: &str = r#"[{"Number":0,"FriendlyName":"Samsung SSD 980","Size":512110190592,"BusType":"NVMe","IsBoot":true,"IsSystem":true,"IsOffline":false},{"Number":2,"FriendlyName":"SanDisk Cruzer \"Blade\"","Size":15489564672,"BusType":"USB","IsBoot":false,"IsSystem":false,"IsOffline":false}]"#;

    #[test]
    fn the_boot_disk_is_not_offered_and_the_stick_is() {
        let devices: Vec<_> = parse_disks(DISKS)
            .into_iter()
            .filter_map(|d| d.into_device(&[]))
            .collect();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].node, PathBuf::from(r"\\.\PhysicalDrive2"));
        assert_eq!(devices[0].size, 15_489_564_672);
        assert_eq!(devices[0].bus, Bus::Usb);
    }

    #[test]
    fn a_quoted_character_in_a_model_name_survives() {
        let devices: Vec<_> = parse_disks(DISKS)
            .into_iter()
            .filter_map(|d| d.into_device(&[]))
            .collect();
        assert_eq!(devices[0].model, r#"SanDisk Cruzer "Blade""#);
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object() {
        // A model name with a brace in it would otherwise split one disk into
        // two half-parsed ones.
        let json = r#"[{"Number":1,"FriendlyName":"Odd } Name","Size":8,"BusType":"USB","IsBoot":false,"IsSystem":false}]"#;
        let disks = parse_disks(json);
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].name, "Odd } Name");
    }

    #[test]
    fn a_missing_safety_flag_is_read_as_the_dangerous_answer() {
        // The property is absent on some builds. Absent must not mean "no".
        let json = r#"[{"Number":3,"FriendlyName":"Stick","Size":8,"BusType":"USB"}]"#;
        let disks = parse_disks(json);
        assert!(disks[0].boot && disks[0].system);
        assert!(disks.into_iter().next().unwrap().into_device(&[]).is_none());
    }

    #[test]
    fn an_internal_bus_is_never_offered_even_when_the_flags_say_it_is_free() {
        // A second internal SSD is not a boot disk and not a system disk, and
        // is absolutely not a write target.
        let json = r#"[{"Number":1,"FriendlyName":"Data SSD","Size":2000398934016,"BusType":"SATA","IsBoot":false,"IsSystem":false,"IsOffline":false}]"#;
        let devices: Vec<_> = parse_disks(json)
            .into_iter()
            .filter_map(|d| d.into_device(&[]))
            .collect();
        assert!(devices.is_empty());
    }

    #[test]
    fn a_truncated_reply_yields_no_disks_rather_than_a_wrong_one() {
        let cut = &DISKS[..DISKS.len() / 2];
        for disk in parse_disks(cut) {
            assert!(disk.boot || disk.system || disk.bus != "USB");
        }
    }

    #[test]
    fn the_drive_node_round_trips_back_to_its_number() {
        let device = Device {
            node: PathBuf::from(r"\\.\PhysicalDrive7"),
            model: String::new(),
            size: 1,
            bus: Bus::Usb,
            volumes: Vec::new(),
        };
        assert_eq!(disk_number(&device).unwrap(), 7);
    }
}
