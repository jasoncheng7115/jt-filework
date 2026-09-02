//! Asking each platform what disks it has.
//!
//! Three implementations of one question, and the same rule in all three: a
//! disk is added to the list only when every property needed to judge it was
//! read successfully and every one of them says removable, external and not the
//! system's. A disk whose properties could not be read is skipped, silently and
//! deliberately — the alternative is offering someone their own boot disk
//! because a parse failed.
//!
//! All three shell out to the platform's own tool rather than calling its C
//! API. That keeps this crate inside the project's no-`unsafe` rule
//! (`AGENTS.md` §20.1), and the cost is a process launch on a dialog the user
//! opened on purpose.

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
use crate::Device;
use jtf_core::Error;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
#[allow(unreachable_pub)]
pub use macos::{is_supported, list, unmount_volumes};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
#[allow(unreachable_pub)]
pub use linux::{is_supported, list, unmount_volumes};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
#[allow(unreachable_pub)]
pub use windows::{is_supported, list, unmount_volumes};

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod elsewhere {
    use super::{Device, Error};

    /// Whether this platform can enumerate and write removable disks.
    pub const fn is_supported() -> bool {
        false
    }

    /// No disks, because this platform's enumeration is not written.
    ///
    /// # Errors
    ///
    /// Always [`jtf_core::ErrorCode::Unsupported`].
    pub fn list() -> Result<Vec<Device>, Error> {
        Err(crate::unsupported("listing removable disks"))
    }

    /// # Errors
    ///
    /// Always [`jtf_core::ErrorCode::Unsupported`].
    pub fn unmount_volumes(_device: &Device) -> Result<(), Error> {
        Err(crate::unsupported("unmounting a disk"))
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub use elsewhere::{is_supported, list, unmount_volumes};

/// Run a system tool and return its standard output.
///
/// Shared by all three platforms. A tool that fails is an error rather than an
/// empty list, because "the tool is missing" and "there are no removable disks"
/// must not look the same to the caller.
///
/// # Errors
///
/// [`jtf_core::ErrorCode::ProviderFailed`] if the tool could not be run, exited
/// non-zero, or wrote something that is not UTF-8.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", target_os = "windows")),
    allow(dead_code)
)]
pub(crate) fn run(program: &str, args: &[&str]) -> Result<String, Error> {
    use jtf_core::ErrorCode;

    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| {
            Error::new(
                ErrorCode::ProviderFailed,
                format!("could not run {program}: {e}"),
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(
            ErrorCode::ProviderFailed,
            format!("{program} failed: {}", stderr.trim()),
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        Error::new(
            ErrorCode::ParseFailed,
            format!("{program} wrote output that is not UTF-8"),
        )
    })
}
