//! Getting a writable handle on a raw disk.
//!
//! Writing to sector zero of a block device needs a privilege a desktop
//! application does not have and should not keep. Each platform has its own
//! way to borrow it for one operation, and this module uses the platform's own
//! rather than inventing one:
//!
//! - **macOS** — `authopen`, a setuid tool that ships with the system. It shows
//!   the standard authorization sheet, opens the file, and copies its standard
//!   input into it. Nothing of ours ever runs as root, which is the strongest
//!   version of this that exists on any of the three platforms.
//! - **Linux** — `pkexec` running `dd`. Polkit shows the desktop's own password
//!   prompt, and `dd` is doing exactly what the user was told it would. Where
//!   the caller already has access to the device — root, or a member of the
//!   `disk` group — the device is opened directly and nothing is prompted.
//! - **Windows** — the device is opened directly. Windows has no equivalent of
//!   a pipe to a privileged writer, so the write runs in an elevated copy of
//!   this program started with the `runas` verb; see [`needs_elevation`].
//!
//! In every case the bytes come from this process, so progress, cancellation
//! and the checksum are computed here rather than being reported by something
//! else.

use std::io::Write;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::{Child, Command, Stdio};
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use std::process::Child;

use jtf_core::{Error, ErrorCode};

use crate::Device;

/// A disk open for writing.
///
/// Dropping this without calling [`Sink::finish`] abandons the write: on the
/// piped platforms the helper is killed, which leaves the disk with whatever
/// arrived so far. That is the correct behaviour for a cancellation — there is
/// no partially-written state worth preserving — but it does mean `finish` is
/// the only way to learn that the helper was happy.
pub struct Sink {
    inner: Inner,
}

enum Inner {
    /// Bytes go to a helper's standard input.
    ///
    /// Not constructed on Windows, which has no way to pass a privileged
    /// descriptor down a pipe; the variant stays so the two paths are one type
    /// and the engine above never learns which platform it is on.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Piped { child: Child, what: &'static str },
    /// Bytes go straight to the device.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Direct(std::fs::File),
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            Inner::Piped { child, what } => match child.stdin.as_mut() {
                Some(stdin) => stdin.write(buf),
                None => Err(std::io::Error::other(format!("{what} has no input"))),
            },
            Inner::Direct(file) => file.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.inner {
            Inner::Piped { child, .. } => match child.stdin.as_mut() {
                Some(stdin) => stdin.flush(),
                None => Ok(()),
            },
            Inner::Direct(file) => {
                file.flush()?;
                // A flush on a file handle empties this program's buffer. The
                // kernel's own cache still holds the tail, and the user is
                // about to pull the disk out.
                file.sync_all()
            }
        }
    }
}

impl Sink {
    /// Close the disk and wait for the helper to say it succeeded.
    ///
    /// Must be called. Everything up to here can succeed while the write still
    /// failed: the helper reports its verdict on exit, and on the direct path
    /// the final sync is where a full or failing disk finally admits it.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::ProviderFailed`] if the helper exited non-zero — which is
    /// what a refused authorization looks like — and [`ErrorCode::Io`] if the
    /// final flush failed.
    pub fn finish(mut self) -> Result<(), Error> {
        self.flush()
            .map_err(|e| Error::new(ErrorCode::Io, format!("finishing the write: {e}")))?;
        match self.inner {
            Inner::Direct(_) => Ok(()),
            Inner::Piped { mut child, what } => {
                // Closing the pipe is what tells the helper there is no more
                // input. Without this it waits for EOF that never comes and
                // the program hangs on a disk it has already written.
                drop(child.stdin.take());
                let status = child.wait().map_err(|e| {
                    Error::new(ErrorCode::ProviderFailed, format!("{what}: {e}"))
                })?;
                if status.success() {
                    Ok(())
                } else {
                    Err(Error::new(
                        ErrorCode::ProviderFailed,
                        format!("{what} exited with {status}"),
                    ))
                }
            }
        }
    }
}

/// Open `device` for writing from sector zero.
///
/// The disk should already have been unmounted with
/// [`crate::unmount_volumes`]; a mounted volume's filesystem driver writes to
/// the same sectors and the two interleave.
///
/// # Errors
///
/// [`ErrorCode::PermissionDenied`] if the privilege could not be obtained,
/// which includes the user declining the prompt; [`ErrorCode::Unsupported`] on
/// a platform with no implementation.
pub fn open(device: &Device) -> Result<Sink, Error> {
    let node = device.node.to_str().ok_or_else(|| {
        Error::new(
            ErrorCode::InvalidPath,
            "the device node is not valid UTF-8",
        )
    })?;
    open_node(node)
}

#[cfg(target_os = "macos")]
fn open_node(node: &str) -> Result<Sink, Error> {
    // -w: open for writing. authopen then copies its stdin to the file it
    // opened, so this process never holds the privileged descriptor.
    spawn("authopen", &["-w", node], "authopen")
}

#[cfg(target_os = "linux")]
fn open_node(node: &str) -> Result<Sink, Error> {
    // Already permitted - running as root, or a member of the disk group - so
    // there is nothing to ask anyone about.
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(node) {
        return Ok(Sink {
            inner: Inner::Direct(file),
        });
    }
    // `conv=fsync` so dd's exit status reflects the data reaching the disk
    // rather than reaching the kernel's cache.
    let of = format!("of={node}");
    spawn(
        "pkexec",
        &["dd", of.as_str(), "bs=4M", "conv=fsync"],
        "pkexec dd",
    )
}

#[cfg(target_os = "windows")]
fn open_node(node: &str) -> Result<Sink, Error> {
    // Windows has no way to hand a privileged descriptor down a pipe, so the
    // write runs in an elevated copy of this program and this is the copy that
    // has the privilege - or does not, in which case the caller is told to
    // relaunch rather than being left with a half-open disk.
    std::fs::OpenOptions::new()
        .write(true)
        .open(node)
        .map(|file| Sink {
            inner: Inner::Direct(file),
        })
        .map_err(|e| {
            let code = if e.kind() == std::io::ErrorKind::PermissionDenied {
                ErrorCode::PermissionDenied
            } else {
                ErrorCode::Io
            };
            Error::new(code, format!("opening {node}: {e}"))
        })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn open_node(_node: &str) -> Result<Sink, Error> {
    Err(crate::unsupported("writing to a raw disk"))
}

/// Start a helper with its standard input piped to us.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn spawn(program: &str, args: &[&str], what: &'static str) -> Result<Sink, Error> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        // The helper's own output is not interesting and must not land in the
        // terminal the application was started from.
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorCode::PermissionDenied,
                format!("could not start {what}: {e}"),
            )
        })?;
    Ok(Sink {
        inner: Inner::Piped { child, what },
    })
}

/// Open `device` for reading back what was just written.
///
/// Reading a raw disk needs the same privilege as writing one, and gets it the
/// same way — except that a read cannot be done down a pipe from `authopen`,
/// so on macOS this reads through the *buffered* node, which an ordinary user
/// can open when the disk has no mounted volumes.
///
/// # Errors
///
/// [`ErrorCode::PermissionDenied`] if the disk could not be opened for reading.
pub fn open_for_read(device: &Device) -> Result<std::fs::File, Error> {
    let node = device.node.to_str().ok_or_else(|| {
        Error::new(ErrorCode::InvalidPath, "the device node is not valid UTF-8")
    })?;
    std::fs::File::open(node).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::Io
        };
        Error::new(code, format!("reading back {node}: {e}"))
    })
}

/// Whether the write has to happen in a separately elevated process.
///
/// True only on Windows, and only when this process is not already elevated.
/// The other two platforms borrow the privilege for the one operation and hand
/// it straight back, which is better and is why they do not need this.
pub fn needs_elevation() -> bool {
    #[cfg(target_os = "windows")]
    {
        // Asked by trying: a test for "am I an administrator" that does not
        // involve opening something is a Windows API call, and this crate does
        // not make any. Opening the first physical drive read-only succeeds for
        // an administrator and fails for everyone else, and touches nothing.
        std::fs::File::open(r"\\.\PhysicalDrive0").is_err()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bus, Device};
    use std::path::PathBuf;

    #[test]
    fn a_device_node_that_is_not_text_is_refused_before_anything_is_opened() {
        // The guard exists so that a decoding failure can never become an open
        // of something else. Tested with a genuinely undecodable node rather
        // than a merely absent one - an absent node on Linux falls through to
        // pkexec, and a test must never put a password prompt on the screen.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            let node = std::ffi::OsStr::from_bytes(b"/dev/\xFF\xFEnot-utf8");
            let device = Device {
                node: PathBuf::from(node),
                model: "test".into(),
                size: 1,
                bus: Bus::Usb,
                volumes: Vec::new(),
            };
            match open(&device) {
                Ok(_) => panic!("an undecodable device node was opened"),
                Err(e) => assert_eq!(e.code(), ErrorCode::InvalidPath),
            }
        }
    }

    #[test]
    fn reading_back_a_device_that_is_not_there_fails_rather_than_returning_nothing() {
        let device = Device {
            node: PathBuf::from("/dev/definitely-not-a-disk"),
            model: "test".into(),
            size: 1,
            bus: Bus::Usb,
            volumes: Vec::new(),
        };
        assert!(open_for_read(&device).is_err());
    }

    #[test]
    fn only_windows_ever_asks_for_a_separate_elevated_process() {
        if !cfg!(target_os = "windows") {
            assert!(!needs_elevation());
        }
    }
}
