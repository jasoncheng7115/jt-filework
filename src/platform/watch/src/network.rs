//! Whether a folder is on a filesystem whose changes this machine cannot see.
//!
//! inotify and FSEvents report what the kernel on *this* machine did. A file
//! written to an NFS or SMB share by another computer never passes through it,
//! so a watch on that share is silent about exactly the changes it exists to
//! report. `KDirWatch`, which Dolphin uses, checks the filesystem type for the
//! same reason and polls those mounts instead; this is that check.
//!
//! Windows is the exception: `ReadDirectoryChangesW` on a share is forwarded to
//! the server as an SMB change notification, which is how Explorer stays
//! current on a mapped drive. Nothing there is polled for being remote.

use std::path::Path;

/// Whether `folder` lives on a network or user-space filesystem, and so should
/// be polled rather than watched. `None` when the filesystem will not say.
///
/// Called on the watcher's own thread, never the UI thread: asking a mount
/// that has stopped answering what it is waits as long as anything else does.
///
/// `None` is not rare enough to guess about. The desktop's document portal,
/// a FUSE mount, refuses `statfs` outright - and a watch on it is accepted and
/// then hears nothing another process does. The caller polls on `None`: an
/// unneeded poll costs a `stat` a second, a watch that cannot hear costs every
/// change.
#[cfg(target_os = "linux")]
pub(crate) fn is_network_filesystem(folder: &Path) -> Option<bool> {
    // statfs(2) and <linux/magic.h>. FUSE is included: sshfs, rclone and
    // davfs2 are the common ones, and a local FUSE filesystem that is polled
    // loses a quarter of a second, where a network one that is watched loses
    // every change.
    const REMOTE: &[i128] = &[
        0x6969,      // NFS
        0x517B,      // SMB
        0xFF53_4D42, // CIFS
        0xFE53_4D42, // SMB2
        0x5346_414F, // AFS
        0x6B41_4653, // kAFS
        0x00C3_6400, // Ceph
        0x7375_7245, // Coda
        0x0102_1997, // 9P
        0x564C,      // NCP
        0x6573_5546, // FUSE
    ];
    let stat = rustix::fs::statfs(folder).ok()?;
    // `f_type` is a signed word whose width differs by architecture, and the
    // CIFS and SMB2 magics have the top bit set; masked to 32 bits they
    // compare the same everywhere.
    let kind = i128::from(stat.f_type) & 0xFFFF_FFFF;
    Some(REMOTE.contains(&kind))
}

/// See the Linux version. macOS says it directly: a local filesystem carries
/// `MNT_LOCAL`, and SMB, NFS, AFP, WebDAV and macFUSE mounts do not.
#[cfg(target_os = "macos")]
pub(crate) fn is_network_filesystem(folder: &Path) -> Option<bool> {
    const MNT_LOCAL: u32 = 0x0000_1000;
    rustix::fs::statfs(folder)
        .ok()
        .map(|stat| stat.f_flags & MNT_LOCAL == 0)
}

/// Windows watches shares natively (see the module comment); elsewhere there
/// is no way to ask, so the watch is tried.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) const fn is_network_filesystem(_folder: &Path) -> Option<bool> {
    Some(false)
}
