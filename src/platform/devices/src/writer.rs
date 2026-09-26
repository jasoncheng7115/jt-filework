//! Getting a writable handle on a raw disk.
//!
//! Writing to sector zero of a block device needs a privilege a desktop
//! application does not have and should not keep. Each platform has its own
//! way to borrow it for one operation, and this module uses the platform's own
//! rather than inventing one:
//!
//! - **macOS** — `authopen`, a setuid tool that ships with the system. It shows
//!   the standard authorization sheet, opens the disk read-write, and hands the
//!   open descriptor back over a socket (`-stdoutpipe`). Nothing of ours ever
//!   runs as root, which is the strongest version of this that exists on any
//!   of the three platforms - and the one descriptor both writes the image and
//!   reads it back, so the password is asked for once. The disk's own node is
//!   `root:operator 0640`; an ordinary user cannot open it to read back, which
//!   is why verifying failed on every Mac until 0.6.54.
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
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use std::process::Child;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::{Child, Command, Stdio};

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
    #[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(dead_code))]
    Piped { child: Child, what: &'static str },
    /// Bytes go straight to the device. `readable` when the descriptor was
    /// opened read-write, so the same one can read the disk back.
    Direct { file: std::fs::File, readable: bool },
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match &mut self.inner {
            Inner::Piped { child, what } => match child.stdin.as_mut() {
                Some(stdin) => stdin.write(buf),
                None => Err(std::io::Error::other(format!("{what} has no input"))),
            },
            Inner::Direct { file, .. } => file.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.inner {
            Inner::Piped { child, .. } => match child.stdin.as_mut() {
                Some(stdin) => stdin.flush(),
                None => Ok(()),
            },
            Inner::Direct { file, .. } => {
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
    /// Finish the write and wait for the helper to say it succeeded.
    ///
    /// Must be called. Everything up to here can succeed while the write still
    /// failed: the helper reports its verdict on exit, and on the direct path
    /// the final sync is where a full or failing disk finally admits it.
    ///
    /// Returns the descriptor itself when it can also read, so the disk is
    /// read back through the access the write already had rather than by
    /// asking for it again - which on macOS is not something this process can
    /// get any other way.
    ///
    /// # Errors
    ///
    /// [`ErrorCode::ProviderFailed`] if the helper exited non-zero — which is
    /// what a refused authorization looks like — and [`ErrorCode::Io`] if the
    /// final flush failed.
    pub fn finish(mut self) -> Result<Option<std::fs::File>, Error> {
        self.flush()
            .map_err(|e| Error::new(ErrorCode::Io, format!("finishing the write: {e}")))?;
        match self.inner {
            Inner::Direct { file, readable } => Ok(readable.then_some(file)),
            Inner::Piped { mut child, what } => {
                // Closing the pipe is what tells the helper there is no more
                // input. Without this it waits for EOF that never comes and
                // the program hangs on a disk it has already written.
                drop(child.stdin.take());
                let status = child
                    .wait()
                    .map_err(|e| Error::new(ErrorCode::ProviderFailed, format!("{what}: {e}")))?;
                if status.success() {
                    Ok(None)
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
    let node = device
        .node
        .to_str()
        .ok_or_else(|| Error::new(ErrorCode::InvalidPath, "the device node is not valid UTF-8"))?;
    open_node(node)
}

/// Where `authopen` actually is.
///
/// By absolute path, not by name. It lives in `/usr/libexec`, which is on
/// nobody's `PATH` - and an application launched from Finder has barely any
/// `PATH` at all - so `Command::new("authopen")` failed to start before it
/// could ask anyone anything. Pressing Write did nothing visible and no
/// authorization sheet ever appeared.
#[cfg(target_os = "macos")]
const AUTHOPEN: &str = "/usr/libexec/authopen";

#[cfg(target_os = "macos")]
fn open_node(node: &str) -> Result<Sink, Error> {
    Ok(Sink {
        inner: Inner::Direct {
            file: authopen_read_write(node)?,
            readable: true,
        },
    })
}

/// Ask `authopen` for `node` open read-write, and take the descriptor it opens.
///
/// `-stdoutpipe` makes it send the descriptor back over its standard output,
/// which is one end of a socket pair, rather than copying data through a pipe;
/// `-o 2` is `O_RDWR`, numerically, as the flag wants it. This is how Apple
/// intends `authopen` to be used from a program, and what Raspberry Pi Imager
/// does. The previous way - `-w`, copying our output through its input - could
/// write and never read, and the read-back then had to open the disk itself,
/// which an ordinary user on macOS cannot.
///
/// A refused or cancelled authorization sends nothing and exits non-zero.
#[cfg(target_os = "macos")]
fn authopen_read_write(node: &str) -> Result<std::fs::File, Error> {
    let flags = rustix::fs::OFlags::RDWR.bits().to_string();
    receive_descriptor(AUTHOPEN, &["-stdoutpipe", "-o", flags.as_str(), node], node)
}

/// Run `program` with a socket as its standard output and take the one
/// descriptor it sends back. Split from `authopen_read_write` so the exchange
/// can be tested without an authorization prompt.
#[cfg(target_os = "macos")]
fn receive_descriptor(program: &str, args: &[&str], node: &str) -> Result<std::fs::File, Error> {
    use rustix::net::{recvmsg, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags};
    use std::io::{IoSliceMut, Read as _};
    use std::mem::MaybeUninit;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

    let (ours, theirs) = UnixStream::pair()
        .map_err(|e| Error::new(ErrorCode::Io, format!("socket pair for authopen: {e}")))?;
    // Spawned from a temporary, so the command - and with it this process's
    // copy of the child's end - is dropped before the receive below. Kept, it
    // would hold the socket open and a refused authorization would never read
    // as an end of file.
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(theirs)))
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                ErrorCode::PermissionDenied,
                format!("could not start authopen: {e}"),
            )
        })?;

    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let mut byte = [0_u8; 1];
    let received = recvmsg(
        &ours,
        &mut [IoSliceMut::new(&mut byte)],
        &mut control,
        RecvFlags::empty(),
    );
    let descriptor = received.ok().and_then(|_| {
        control.drain().find_map(|message| match message {
            RecvAncillaryMessage::ScmRights(mut fds) => fds.next(),
            _ => None,
        })
    });

    let status = child
        .wait()
        .map_err(|e| Error::new(ErrorCode::PermissionDenied, format!("authopen: {e}")))?;
    match descriptor {
        Some(fd) if status.success() => Ok(std::fs::File::from(fd)),
        _ => {
            let mut said = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut said);
            }
            Err(Error::new(
                ErrorCode::PermissionDenied,
                format!("authopen did not open {node} ({status}): {}", said.trim()),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
fn open_node(node: &str) -> Result<Sink, Error> {
    // Already permitted - running as root, or a member of the disk group - so
    // there is nothing to ask anyone about.
    // Write-only, and read back by reopening: a block device read through the
    // descriptor that just wrote it can come from the page cache.
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(node) {
        return Ok(Sink {
            inner: Inner::Direct {
                file,
                readable: false,
            },
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
            inner: Inner::Direct {
                file,
                readable: false,
            },
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
#[cfg(target_os = "linux")]
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
    let node = device
        .node
        .to_str()
        .ok_or_else(|| Error::new(ErrorCode::InvalidPath, "the device node is not valid UTF-8"))?;
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

#[cfg(all(test, target_os = "macos"))]
mod descriptor_tests {
    use std::io::{Read, Seek, SeekFrom, Write};

    /// A stand-in for `authopen -stdoutpipe`: opens the file named last,
    /// read-write, and sends the descriptor down its standard output.
    const SENDER: &str = "import os, socket, sys\n\
                          s = socket.socket(fileno=1)\n\
                          fd = os.open(sys.argv[-1], os.O_RDWR)\n\
                          socket.send_fds(s, [b'x'], [fd])\n";

    fn python() -> Option<&'static str> {
        [
            "/usr/bin/python3",
            "/opt/homebrew/bin/python3",
            "/usr/local/bin/python3",
        ]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
    }

    /// The descriptor that arrives is the file, open both ways: what is
    /// written through it can be read back through it.
    #[test]
    fn the_descriptor_sent_back_reads_and_writes_the_file() {
        let Some(python) = python() else { return };
        let path = std::env::temp_dir().join(format!("jtf-fd-{}", std::process::id()));
        std::fs::write(&path, b"before").unwrap();
        let node = path.to_str().unwrap();
        let mut file =
            super::receive_descriptor(python, &["-c", SENDER, node], node).expect("a descriptor");
        file.write_all(b"AFTER!").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut back = String::new();
        file.read_to_string(&mut back).unwrap();
        assert_eq!(back, "AFTER!");
        let _ = std::fs::remove_file(path);
    }

    /// A refused authorization sends nothing and exits non-zero. That is a
    /// refusal, said as one - not a hang waiting for a descriptor that is not
    /// coming, and not a success with nothing to write to.
    #[test]
    fn a_helper_that_sends_nothing_is_a_refusal_not_a_hang() {
        let Some(python) = python() else { return };
        let error = super::receive_descriptor(
            python,
            &["-c", "import sys; sys.exit(1)", "/dev/null"],
            "/dev/null",
        )
        .expect_err("nothing was sent");
        assert_eq!(error.code(), jtf_core::ErrorCode::PermissionDenied);
    }
}

#[cfg(test)]
mod path_tests {
    /// The helper is named by absolute path and that path is the real one.
    ///
    /// A bare name is resolved against `PATH`, which a bundled application
    /// does not meaningfully have - this is the whole of the bug this test
    /// exists for, and it is not visible from the outside because a helper
    /// that never starts looks the same as one the user declined.
    #[cfg(target_os = "macos")]
    #[test]
    fn authopen_is_named_by_absolute_path_and_is_there() {
        let path = std::path::Path::new(super::AUTHOPEN);
        assert!(
            path.is_absolute(),
            "resolved against PATH: {}",
            super::AUTHOPEN
        );
        assert!(
            path.exists(),
            "not where we look for it: {}",
            super::AUTHOPEN
        );
    }
}
